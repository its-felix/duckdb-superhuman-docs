#include "storage/rust_bridge_dml.hpp"

#include "rust_bridge_extension.h"
#include "duckdb/common/exception.hpp"
#include "duckdb/main/client_context.hpp"
#include "duckdb/planner/operator/logical_delete.hpp"
#include "duckdb/transaction/transaction.hpp"
#include "storage/rust_bridge_catalog.hpp"
#include "storage/rust_bridge_transaction.hpp"

namespace duckdb {

class RustBridgeDMLGlobalState : public GlobalSinkState {
public:
	RustBridgeDMLGlobalState() : affected_count(0) {
	}

	mutex lock;
	idx_t affected_count;
};

static RustBridgeClient GetClient(const RustBridgeTableCatalogEntry &table) {
	auto &catalog = table.catalog.Cast<RustBridgeCatalog>();
	return catalog.Client();
}

static void AddAffected(RustBridgeDMLGlobalState &state, idx_t count) {
	lock_guard<mutex> lock(state.lock);
	state.affected_count += count;
}

static void MarkRemoteWrite(ClientContext &context, const RustBridgeTableCatalogEntry &table) {
	auto &transaction = Transaction::Get(context, table.catalog.GetAttached()).Cast<RustBridgeTransaction>();
	transaction.MarkWrite();
}

RustBridgeDML::RustBridgeDML(PhysicalPlan &physical_plan, LogicalOperator &op, idx_t estimated_cardinality)
    : PhysicalOperator(physical_plan, PhysicalOperatorType::EXTENSION, op.types, estimated_cardinality) {
}

unique_ptr<GlobalSinkState> RustBridgeDML::GetGlobalSinkState(ClientContext &) const {
	return make_uniq<RustBridgeDMLGlobalState>();
}

SinkCombineResultType RustBridgeDML::Combine(ExecutionContext &, OperatorSinkCombineInput &) const {
	return SinkCombineResultType::FINISHED;
}

SourceResultType RustBridgeDML::GetDataInternal(ExecutionContext &, DataChunk &chunk, OperatorSourceInput &) const {
	auto &state = sink_state->Cast<RustBridgeDMLGlobalState>();
	chunk.data[0].SetValue(0, Value::BIGINT(NumericCast<int64_t>(state.affected_count)));
#ifdef __EMSCRIPTEN__
	chunk.SetCardinality(1);
#else
	chunk.SetCardinalityUnsafe(1);
#endif
	return SourceResultType::FINISHED;
}

RustBridgeInsert::RustBridgeInsert(PhysicalPlan &physical_plan, LogicalOperator &op,
                                   RustBridgeTableCatalogEntry &table_p)
    : RustBridgeDML(physical_plan, op, 1), table(table_p) {
}

SinkResultType RustBridgeInsert::Sink(ExecutionContext &context, DataChunk &chunk, OperatorSinkInput &input) const {
	if (chunk.size() == 0) {
		return SinkResultType::NEED_MORE_INPUT;
	}
	RustBridgeRejectExplicitTransaction(context.client);
	if (!table->TableInfo().Supports(RUST_EXT_TABLE_INSERT)) {
		throw NotImplementedException("%s", rust_ext_dml_not_supported_message(RUST_EXT_DML_INSERT));
	}
	auto client = GetClient(*table);
	auto count = client.InsertRows(table->TableInfo(), chunk);
	MarkRemoteWrite(context.client, *table);
	AddAffected(input.global_state.Cast<RustBridgeDMLGlobalState>(), count);
	return SinkResultType::NEED_MORE_INPUT;
}

string RustBridgeInsert::GetName() const {
	return rust_ext_insert_operator_name();
}

RustBridgeUpdate::RustBridgeUpdate(PhysicalPlan &physical_plan, LogicalUpdate &op, RustBridgeTableCatalogEntry &table_p)
    : RustBridgeDML(physical_plan, op, op.estimated_cardinality), table(table_p), columns(std::move(op.columns)),
      expressions(std::move(op.expressions)) {
}

SinkResultType RustBridgeUpdate::Sink(ExecutionContext &context, DataChunk &chunk, OperatorSinkInput &input) const {
	if (chunk.size() == 0) {
		return SinkResultType::NEED_MORE_INPUT;
	}
	RustBridgeRejectExplicitTransaction(context.client);
	if (!table->TableInfo().Supports(RUST_EXT_TABLE_UPDATE)) {
		throw NotImplementedException("%s", rust_ext_dml_not_supported_message(RUST_EXT_DML_UPDATE));
	}
	auto client = GetClient(*table);
	auto count = client.UpdateRows(table->TableInfo(), chunk, columns, expressions);
	MarkRemoteWrite(context.client, *table);
	AddAffected(input.global_state.Cast<RustBridgeDMLGlobalState>(), count);
	return SinkResultType::NEED_MORE_INPUT;
}

string RustBridgeUpdate::GetName() const {
	return rust_ext_update_operator_name();
}

RustBridgeDelete::RustBridgeDelete(PhysicalPlan &physical_plan, LogicalDelete &op, RustBridgeTableCatalogEntry &table_p,
                                   idx_t row_id_index_p)
    : RustBridgeDML(physical_plan, op, op.estimated_cardinality), table(table_p), row_id_index(row_id_index_p) {
}

SinkResultType RustBridgeDelete::Sink(ExecutionContext &context, DataChunk &chunk, OperatorSinkInput &input) const {
	if (chunk.size() == 0) {
		return SinkResultType::NEED_MORE_INPUT;
	}
	RustBridgeRejectExplicitTransaction(context.client);
	if (!table->TableInfo().Supports(RUST_EXT_TABLE_DELETE)) {
		throw NotImplementedException("%s", rust_ext_dml_not_supported_message(RUST_EXT_DML_DELETE));
	}
	auto client = GetClient(*table);
	auto count = client.DeleteRows(table->TableInfo(), chunk, row_id_index);
	MarkRemoteWrite(context.client, *table);
	AddAffected(input.global_state.Cast<RustBridgeDMLGlobalState>(), count);
	return SinkResultType::NEED_MORE_INPUT;
}

string RustBridgeDelete::GetName() const {
	return rust_ext_delete_operator_name();
}

} // namespace duckdb
