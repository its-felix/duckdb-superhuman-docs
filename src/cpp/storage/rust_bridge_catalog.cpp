#include "storage/rust_bridge_catalog.hpp"

#include "rust_bridge_string.hpp"
#include "duckdb/common/exception.hpp"
#include "duckdb/common/string_util.hpp"
#include "duckdb/execution/physical_plan_generator.hpp"
#include "duckdb/main/client_context.hpp"
#include "duckdb/parser/parsed_data/create_schema_info.hpp"
#include "duckdb/parser/parsed_data/drop_info.hpp"
#include "duckdb/storage/database_size.hpp"

namespace duckdb {

RustBridgeCatalog::RustBridgeCatalog(AttachedDatabase &db, ClientContext &context,
                                     RustBridgeAttachConfig attach_config_p)
    : Catalog(db), attach_config(std::move(attach_config_p)) {
	LoadCatalog(context);
}

RustBridgeCatalog::~RustBridgeCatalog() {
}

void RustBridgeCatalog::LoadCatalog(ClientContext &context) {
	auto client = Client();
	catalog_info = client.ListTables();

	CreateSchemaInfo schema_info;
	main_schema = make_uniq<RustBridgeSchemaCatalogEntry>(context, *this, schema_info, catalog_info);
}

RustBridgeClient RustBridgeCatalog::Client() const {
	return RustBridgeClient(attach_config.ClientConfig());
}

void RustBridgeCatalog::Initialize(bool) {
}

optional_ptr<SchemaCatalogEntry> RustBridgeCatalog::LookupSchema(CatalogTransaction,
                                                                 const EntryLookupInfo &schema_lookup,
                                                                 OnEntryNotFound if_not_found) {
	if (StringUtil::CIEquals(schema_lookup.GetEntryName(), DEFAULT_SCHEMA)) {
		return main_schema.get();
	}
	if (if_not_found == OnEntryNotFound::THROW_EXCEPTION) {
		throw BinderException("Schema with name \"%s\" not found", schema_lookup.GetEntryName());
	}
	return nullptr;
}

void RustBridgeCatalog::ScanSchemas(ClientContext &, std::function<void(SchemaCatalogEntry &)> callback) {
	callback(*main_schema);
}

optional_ptr<CatalogEntry> RustBridgeCatalog::CreateSchema(CatalogTransaction, CreateSchemaInfo &) {
	throw NotImplementedException("%s", rust_ext_ddl_not_supported_message(RUST_EXT_DDL_CREATE_SCHEMA));
}

void RustBridgeCatalog::DropSchema(ClientContext &, DropInfo &) {
	throw NotImplementedException("%s", rust_ext_ddl_not_supported_message(RUST_EXT_DDL_DROP_SCHEMA));
}

DatabaseSize RustBridgeCatalog::GetDatabaseSize(ClientContext &) {
	throw NotImplementedException("%s", rust_ext_database_size_not_available_message());
}

bool RustBridgeCatalog::InMemory() {
	return false;
}

string RustBridgeCatalog::GetDBPath() {
	return RustBridgeString(attach_config.DatabaseName());
}

} // namespace duckdb
