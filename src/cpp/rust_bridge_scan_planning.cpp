#include "rust_bridge_scan_planning.hpp"

#include "rust_bridge_extension.h"
#include "rust_bridge_scan.hpp"
#include "rust_bridge_string.hpp"
#include "rust_bridge_value.hpp"
#include "duckdb/planner/expression/bound_columnref_expression.hpp"
#include "duckdb/planner/expression/bound_comparison_expression.hpp"
#include "duckdb/planner/expression/bound_constant_expression.hpp"
#include "duckdb/planner/operator/logical_get.hpp"
#include "duckdb/storage/table/row_group_reorderer.hpp"

namespace duckdb {

static bool TryExtractRustBridgeEqualityFilter(const LogicalGet &get, const RustBridgeTableInfo &table,
                                               const Expression &expr, string &query, string &description) {
#ifdef __EMSCRIPTEN__
	if (expr.GetExpressionType() != ExpressionType::COMPARE_EQUAL ||
	    expr.GetExpressionClass() != ExpressionClass::BOUND_COMPARISON) {
		return false;
	}

	auto &comparison = expr.Cast<BoundComparisonExpression>();
	auto left = comparison.left.get();
	auto right = comparison.right.get();
#else
	if (expr.GetExpressionType() != ExpressionType::COMPARE_EQUAL || !BoundComparisonExpression::IsComparison(expr)) {
		return false;
	}

	auto &comparison = expr.Cast<BoundFunctionExpression>();
	auto left = &BoundComparisonExpression::Left(comparison);
	auto right = &BoundComparisonExpression::Right(comparison);
#endif
	if (left->GetExpressionClass() != ExpressionClass::BOUND_COLUMN_REF ||
	    right->GetExpressionClass() != ExpressionClass::BOUND_CONSTANT) {
		std::swap(left, right);
	}
	if (left->GetExpressionClass() != ExpressionClass::BOUND_COLUMN_REF ||
	    right->GetExpressionClass() != ExpressionClass::BOUND_CONSTANT) {
		return false;
	}

	auto &column_ref = left->Cast<BoundColumnRefExpression>();
#ifdef __EMSCRIPTEN__
	auto &binding = column_ref.binding;
#else
	auto &binding = column_ref.Binding();
#endif
	if (binding.table_index != get.table_index) {
		return false;
	}
	auto &column_ids = get.GetColumnIds();
	if (binding.column_index >= column_ids.size()) {
		return false;
	}
	auto column_index = column_ids[binding.column_index];
	if (column_index.IsVirtualColumn()) {
		return false;
	}
	auto col_idx = column_index.GetPrimaryIndex();
	if (col_idx >= table.columns.size()) {
		return false;
	}
	auto &column = table.columns[col_idx];
	if (!rust_ext_scan_can_filter_equality(column.Raw().handle)) {
		return false;
	}
#ifdef __EMSCRIPTEN__
	auto &constant = right->Cast<BoundConstantExpression>().value;
#else
	auto &constant = right->Cast<BoundConstantExpression>().GetValue();
#endif
	if (constant.IsNull()) {
		return false;
	}

	RustBridgeInputValueBuffer value_buffer;
	auto value = value_buffer.Convert(constant);
	RustExtString query_result;
	RustExtString description_result;
	RustExtError error;
	if (!rust_ext_build_equality_query(column.Raw().handle, value, &query_result, &description_result, &error)) {
		rust_ext_free_error(error);
		return false;
	}
	query = TakeRustBridgeString(query_result);
	description = TakeRustBridgeString(description_result);
	return true;
}

static void RustBridgeScanPushdownComplexFilter(ClientContext &, LogicalGet &get, FunctionData *bind_data_p,
                                                vector<unique_ptr<Expression>> &filters) {
	auto &bind_data = bind_data_p->Cast<RustBridgeScanBindData>();
	if (!bind_data.pushed_query.empty()) {
		return;
	}

	for (idx_t filter_idx = 0; filter_idx < filters.size(); filter_idx++) {
		string query;
		string description;
		if (!TryExtractRustBridgeEqualityFilter(get, bind_data.table, *filters[filter_idx], query, description)) {
			continue;
		}
		bind_data.pushed_query = std::move(query);
		bind_data.pushed_query_description = std::move(description);
		filters.erase_at(filter_idx);
		return;
	}
}

static virtual_column_map_t RustBridgeScanGetVirtualColumns(ClientContext &, optional_ptr<FunctionData> bind_data) {
	virtual_column_map_t result;
	if (!bind_data || bind_data->Cast<RustBridgeScanBindData>().table.Supports(RUST_EXT_TABLE_ROW_ID)) {
		result.insert(
		    make_pair(COLUMN_IDENTIFIER_ROW_ID, TableColumn(rust_ext_row_id_column_name(), LogicalType::VARCHAR)));
	}
	return result;
}

static vector<column_t> RustBridgeScanGetRowIdColumns(ClientContext &, optional_ptr<FunctionData> bind_data) {
	if (bind_data && !bind_data->Cast<RustBridgeScanBindData>().table.Supports(RUST_EXT_TABLE_ROW_ID)) {
		return {};
	}
	return {COLUMN_IDENTIFIER_ROW_ID};
}

static BindInfo RustBridgeScanGetBindInfo(const optional_ptr<FunctionData> bind_data) {
	auto &rust_bridge_bind = bind_data->Cast<RustBridgeScanBindData>();
	return BindInfo(rust_bridge_bind.table_entry);
}

static void RustBridgeScanSetScanOrder(unique_ptr<RowGroupOrderOptions> order_options,
                                       optional_ptr<FunctionData> bind_data_p) {
	auto &bind_data = bind_data_p->Cast<RustBridgeScanBindData>();
	if (!order_options || !order_options->row_limit.IsValid()) {
		return;
	}
	if (order_options->order_type == OrderType::DESCENDING) {
		return;
	}

	auto col_idx = order_options->column_idx.GetPrimaryIndex();
	if (col_idx >= bind_data.table.columns.size()) {
		return;
	}
	auto &column = bind_data.table.columns[col_idx];
	RustExtString sort_by;
	if (!rust_ext_scan_sort_by(column.Raw().handle, &sort_by)) {
		return;
	}
	bind_data.pushed_sort_by = TakeRustBridgeString(sort_by);
	bind_data.pushed_limit = order_options->row_limit.GetIndex();
}

static InsertionOrderPreservingMap<string> RustBridgeScanToString(TableFunctionToStringInput &input) {
	auto &bind_data = input.bind_data->Cast<RustBridgeScanBindData>();
	InsertionOrderPreservingMap<string> result;
	if (!bind_data.pushed_query.empty()) {
		result[rust_ext_scan_query_label()] = bind_data.pushed_query_description;
	}
	if (!bind_data.pushed_sort_by.empty()) {
		result[rust_ext_scan_sort_label()] = bind_data.pushed_sort_by;
	}
	if (bind_data.pushed_limit > 0) {
		result[rust_ext_scan_limit_label()] = to_string(bind_data.pushed_limit);
	}
	return result;
}

void ConfigureRustBridgeScanPlanning(TableFunction &function) {
	function.get_virtual_columns = RustBridgeScanGetVirtualColumns;
	function.get_row_id_columns = RustBridgeScanGetRowIdColumns;
	function.get_bind_info = RustBridgeScanGetBindInfo;
	function.pushdown_complex_filter = RustBridgeScanPushdownComplexFilter;
	function.set_scan_order = RustBridgeScanSetScanOrder;
	function.to_string = RustBridgeScanToString;
	function.projection_pushdown = true;
}

} // namespace duckdb
