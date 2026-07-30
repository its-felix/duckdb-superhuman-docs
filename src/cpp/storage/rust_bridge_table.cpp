#include "storage/rust_bridge_table.hpp"

#include "rust_bridge_scan.hpp"
#include "rust_bridge_string.hpp"
#include "duckdb/parser/parsed_data/create_table_info.hpp"
#include "duckdb/storage/table_storage_info.hpp"
#include "storage/rust_bridge_catalog.hpp"

namespace duckdb {

CreateTableInfo RustBridgeCreateTableInfo(const RustBridgeTableInfo &table, const string &schema_name) {
#ifdef __EMSCRIPTEN__
	CreateTableInfo info(INVALID_CATALOG, schema_name, RustBridgeString(table.Raw().name));
	for (auto &column : table.columns) {
		info.columns.AddColumn(ColumnDefinition(RustBridgeString(column.Raw().name), column.duckdb_type));
	}
#else
	auto table_name = Identifier(RustBridgeString(table.Raw().name));
	CreateTableInfo info(QualifiedName(Identifier::InvalidCatalog(), Identifier(schema_name), std::move(table_name)));
	for (auto &column : table.columns) {
		info.columns.AddColumn(ColumnDefinition(Identifier(RustBridgeString(column.Raw().name)), column.duckdb_type));
	}
#endif
	return std::move(info);
}

RustBridgeTableCatalogEntry::RustBridgeTableCatalogEntry(Catalog &catalog, SchemaCatalogEntry &schema,
                                                         CreateTableInfo &info, RustBridgeTableInfo table_p)
    : TableCatalogEntry(catalog, schema, info), table(std::move(table_p)) {
}

unique_ptr<BaseStatistics> RustBridgeTableCatalogEntry::GetStatistics(ClientContext &, column_t) {
	return nullptr;
}

TableFunction RustBridgeTableCatalogEntry::GetScanFunction(ClientContext &, unique_ptr<FunctionData> &bind_data) {
	bind_data = make_uniq<RustBridgeScanBindData>(*this, table);
	return RustBridgeScanFunction::GetFunction();
}

TableStorageInfo RustBridgeTableCatalogEntry::GetStorageInfo(ClientContext &) {
	return TableStorageInfo();
}

virtual_column_map_t RustBridgeTableCatalogEntry::GetVirtualColumns() const {
	virtual_column_map_t result;
	if (table.Supports(RUST_EXT_TABLE_ROW_ID)) {
		result.insert(
		    make_pair(COLUMN_IDENTIFIER_ROW_ID, TableColumn(rust_ext_row_id_column_name(), LogicalType::VARCHAR)));
	}
	return result;
}

vector<column_t> RustBridgeTableCatalogEntry::GetRowIdColumns() const {
	if (!table.Supports(RUST_EXT_TABLE_ROW_ID)) {
		return {};
	}
	return {COLUMN_IDENTIFIER_ROW_ID};
}

} // namespace duckdb
