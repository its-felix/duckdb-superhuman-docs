#include "storage/rust_bridge_catalog.hpp"

#include "rust_bridge_extension.h"
#include "duckdb/common/exception.hpp"
#include "duckdb/execution/physical_plan_generator.hpp"
#include "duckdb/main/client_context.hpp"
#include "duckdb/planner/expression/bound_reference_expression.hpp"
#include "duckdb/planner/operator/logical_create_table.hpp"
#include "duckdb/planner/operator/logical_delete.hpp"
#include "duckdb/planner/operator/logical_insert.hpp"
#include "duckdb/planner/operator/logical_update.hpp"
#include "storage/rust_bridge_dml.hpp"
#include "storage/rust_bridge_transaction.hpp"

namespace duckdb {

PhysicalOperator &RustBridgeCatalog::PlanInsert(ClientContext &context, PhysicalPlanGenerator &planner,
                                                LogicalInsert &op, optional_ptr<PhysicalOperator> plan) {
	RustBridgeRejectExplicitTransaction(context);
	if (op.return_chunk) {
		throw NotImplementedException("%s", rust_ext_returning_not_supported_message(RUST_EXT_DML_INSERT));
	}
	if (!op.table.Cast<RustBridgeTableCatalogEntry>().TableInfo().Supports(RUST_EXT_TABLE_INSERT)) {
		throw NotImplementedException("%s", rust_ext_dml_not_supported_message(RUST_EXT_DML_INSERT));
	}
	D_ASSERT(plan);
	if (!op.column_index_map.empty()) {
		plan = planner.ResolveDefaultsProjection(op, *plan);
	}
	auto &insert = planner.Make<RustBridgeInsert>(op, op.table.Cast<RustBridgeTableCatalogEntry>());
	insert.children.push_back(*plan);
	return insert;
}

PhysicalOperator &RustBridgeCatalog::PlanCreateTableAs(ClientContext &, PhysicalPlanGenerator &, LogicalCreateTable &,
                                                       PhysicalOperator &) {
	throw NotImplementedException("%s", rust_ext_ddl_not_supported_message(RUST_EXT_DDL_CREATE_TABLE_AS));
}

PhysicalOperator &RustBridgeCatalog::PlanUpdate(ClientContext &context, PhysicalPlanGenerator &planner,
                                                LogicalUpdate &op, PhysicalOperator &plan) {
	RustBridgeRejectExplicitTransaction(context);
	if (op.return_chunk) {
		throw NotImplementedException("%s", rust_ext_returning_not_supported_message(RUST_EXT_DML_UPDATE));
	}
	if (!op.table.Cast<RustBridgeTableCatalogEntry>().TableInfo().Supports(RUST_EXT_TABLE_UPDATE)) {
		throw NotImplementedException("%s", rust_ext_dml_not_supported_message(RUST_EXT_DML_UPDATE));
	}
	auto &update = planner.Make<RustBridgeUpdate>(op, op.table.Cast<RustBridgeTableCatalogEntry>());
	update.children.push_back(plan);
	return update;
}

PhysicalOperator &RustBridgeCatalog::PlanDelete(ClientContext &context, PhysicalPlanGenerator &planner,
                                                LogicalDelete &op, PhysicalOperator &plan) {
	RustBridgeRejectExplicitTransaction(context);
	if (op.return_chunk) {
		throw NotImplementedException("%s", rust_ext_returning_not_supported_message(RUST_EXT_DML_DELETE));
	}
	if (!op.table.Cast<RustBridgeTableCatalogEntry>().TableInfo().Supports(RUST_EXT_TABLE_DELETE)) {
		throw NotImplementedException("%s", rust_ext_dml_not_supported_message(RUST_EXT_DML_DELETE));
	}
	auto &bound_ref = op.expressions[0]->Cast<BoundReferenceExpression>();
#ifdef __EMSCRIPTEN__
	auto &del = planner.Make<RustBridgeDelete>(op, op.table.Cast<RustBridgeTableCatalogEntry>(), bound_ref.index);
#else
	auto &del = planner.Make<RustBridgeDelete>(op, op.table.Cast<RustBridgeTableCatalogEntry>(), bound_ref.Index());
#endif
	del.children.push_back(plan);
	return del;
}

unique_ptr<LogicalOperator> RustBridgeCatalog::BindCreateIndex(Binder &, CreateStatement &, TableCatalogEntry &,
                                                               unique_ptr<LogicalOperator>) {
	throw NotImplementedException("%s", rust_ext_ddl_not_supported_message(RUST_EXT_DDL_CREATE_INDEX));
}

} // namespace duckdb
