#include "rust_bridge_scan.hpp"
#include "rust_bridge_client.hpp"

#include "rust_bridge_extension.h"
#include "rust_bridge_scan_planning.hpp"
#include "rust_bridge_string.hpp"
#include "duckdb/common/exception.hpp"
#include "duckdb/common/types/data_chunk.hpp"
#include "duckdb/common/types/value.hpp"
#include "duckdb/common/vector_size.hpp"
#include "duckdb/function/table_function.hpp"
#include "duckdb/main/client_context.hpp"
#include "storage/rust_bridge_catalog.hpp"

namespace duckdb {

class RustBridgeScanGlobalState : public GlobalTableFunctionState {
public:
	RustBridgeScanGlobalState(ClientContext &, const RustBridgeScanBindData &bind_data, TableFunctionInitInput &input)
	    : client(bind_data.table_entry.catalog.Cast<RustBridgeCatalog>().Client()), table(bind_data.table) {
		if (!input.projection_ids.empty()) {
			column_indexes.reserve(input.projection_ids.size());
			for (auto projection_id : input.projection_ids) {
				column_indexes.push_back(input.column_indexes[projection_id]);
			}
		} else {
			column_indexes = input.column_indexes;
		}
		request.filter = bind_data.pushed_query;
		request.order = bind_data.pushed_sort_by;
		request.limit = bind_data.pushed_limit;
		remaining_rows = bind_data.pushed_limit;
		scan = client.OpenScan(table, request);
	}

	idx_t MaxThreads() const override {
		return 1;
	}

	RustBridgeClient client;
	RustBridgeTableInfo table;
	vector<ColumnIndex> column_indexes;
	RustBridgeScanRequest request;
	RustBridgeScanHandle scan;
	RustBridgeScanBatch rows;
	idx_t row_offset = 0;
	idx_t remaining_rows = 0;
	bool finished = false;
};

static unique_ptr<GlobalTableFunctionState> RustBridgeScanInitGlobal(ClientContext &context,
                                                                     TableFunctionInitInput &input) {
	if (!rust_ext_supports_explicit_transactions() && !context.transaction.IsAutoCommit()) {
		throw NotImplementedException("%s", rust_ext_explicit_transaction_not_supported_message());
	}
	auto &bind_data = input.bind_data->Cast<RustBridgeScanBindData>();
	return make_uniq<RustBridgeScanGlobalState>(context, bind_data, input);
}

static Value ScanTextToValue(const string &value, const LogicalType &target_type) {
	if (target_type.IsJSONType()) {
		auto result = Value(value);
		result.GetTypeMutable() = target_type;
		return result;
	}
	if (target_type.id() == LogicalTypeId::VARCHAR) {
		return Value(value);
	}
	try {
		return Value(value).DefaultCastAs(target_type);
	} catch (...) {
		return Value(target_type);
	}
}

static Value ScanValueToValue(const RustExtScanValue &scan_value, const LogicalType &target_type) {
	if (scan_value.is_null) {
		return Value(target_type);
	}
	if (target_type.id() == LogicalTypeId::LIST) {
		auto &child_type = ListType::GetChildType(target_type);
		vector<Value> values;
		values.reserve(scan_value.array_count);
		for (idx_t idx = 0; idx < scan_value.array_count; idx++) {
			auto &array_value = scan_value.array_values[idx];
			if (array_value.is_null) {
				values.emplace_back(child_type);
			} else {
				values.push_back(ScanTextToValue(RustBridgeString(array_value.value), child_type));
			}
		}
		return Value::LIST(child_type, std::move(values));
	}
	return ScanTextToValue(RustBridgeString(scan_value.value), target_type);
}

static void RustBridgeScan(ClientContext &, TableFunctionInput &input, DataChunk &output) {
	auto &state = input.global_state->Cast<RustBridgeScanGlobalState>();
	if (state.finished && state.row_offset >= state.rows.RowCount()) {
		return;
	}

	idx_t out_row = 0;
	while (out_row < STANDARD_VECTOR_SIZE) {
		if (state.row_offset >= state.rows.RowCount()) {
			if (state.finished) {
				break;
			}
			auto response = state.client.NextScanBatch(state.scan);
			state.rows = std::move(response);
			state.finished = state.rows.Finished();
			state.row_offset = 0;
			if (state.rows.Empty()) {
				if (state.finished) {
					break;
				}
				continue;
			}
		}

		auto &row = state.rows.Raw().rows[state.row_offset++];
		for (idx_t out_col = 0; out_col < state.column_indexes.size(); out_col++) {
			auto column_index = state.column_indexes[out_col];
			if (column_index.IsVirtualColumn()) {
				if (column_index.GetPrimaryIndex() == COLUMN_IDENTIFIER_ROW_ID) {
					output.data[out_col].SetValue(out_row, Value(RustBridgeString(row.row_id)));
				} else {
					output.data[out_col].SetValue(out_row, Value(output.data[out_col].GetType()));
				}
				continue;
			}
			auto col_idx = column_index.GetPrimaryIndex();
			if (col_idx >= state.table.columns.size()) {
				throw InternalException("%s", rust_ext_scan_column_index_out_of_range_message());
			}
			auto &column = state.table.columns[col_idx];
			RustExtScanValue scan_value;
			if (!rust_ext_scan_value(column.Raw().handle, row.handle, &scan_value)) {
				output.data[out_col].SetValue(out_row, Value(column.duckdb_type));
			} else {
				auto value = ScanValueToValue(scan_value, column.duckdb_type);
				rust_ext_free_scan_value(scan_value);
				output.data[out_col].SetValue(out_row, value);
			}
		}
		out_row++;
		if (state.remaining_rows > 0 && --state.remaining_rows == 0) {
			state.finished = true;
			break;
		}
	}
#ifdef __EMSCRIPTEN__
	output.SetCardinality(out_row);
#else
	output.SetCardinalityUnsafe(out_row);
#endif
}

TableFunction RustBridgeScanFunction::GetFunction() {
	TableFunction function(rust_ext_scan_function_name(), {}, RustBridgeScan, nullptr, RustBridgeScanInitGlobal);
	ConfigureRustBridgeScanPlanning(function);
	return function;
}

} // namespace duckdb
