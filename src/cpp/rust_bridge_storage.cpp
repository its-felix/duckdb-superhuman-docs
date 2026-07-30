#include "rust_bridge_storage.hpp"

#include "rust_bridge_extension.h"
#include "rust_bridge_string.hpp"
#include "duckdb/main/attached_database.hpp"
#include "duckdb/main/secret/secret.hpp"
#include "duckdb/main/secret/secret_manager.hpp"
#include "duckdb/parser/parsed_data/attach_info.hpp"
#include "duckdb/storage/storage_extension.hpp"
#include "storage/rust_bridge_catalog.hpp"
#include "storage/rust_bridge_transaction_manager.hpp"

namespace duckdb {

static void AllocateHostString(const string &value, RustExtString *out) {
	RustExtError alloc_error;
	if (!rust_ext_alloc_string(value.c_str(), value.size(), out, &alloc_error)) {
		throw OutOfMemoryException("%s", TakeRustBridgeErrorMessage(alloc_error));
	}
}

struct AttachHostContext {
	ClientContext &context;
	AttachOptions &attach_options;
};

static bool HostGetAttachOption(void *userdata, const char *name, RustExtString *out, RustExtError *err) {
	try {
		auto &context = *reinterpret_cast<AttachHostContext *>(userdata);
		auto entry = context.attach_options.options.find(name);
		if (entry == context.attach_options.options.end() || entry->second.IsNull()) {
			*out = RustExtString {};
			return true;
		}
		auto value = entry->second.ToString();
		AllocateHostString(value, out);
		return true;
	} catch (std::exception &ex) {
		SetRustBridgeErrorMessage(err, ex.what());
		return false;
	}
}

static bool HostLookupSecret(void *userdata, RustExtString scope, const char *secret_type, const char *secret_key,
                             RustExtString *out, RustExtError *err) {
	try {
		auto &context = *reinterpret_cast<AttachHostContext *>(userdata);
		auto &secret_manager = SecretManager::Get(context.context);
		auto transaction = CatalogTransaction::GetSystemCatalogTransaction(context.context);
		auto match = secret_manager.LookupSecret(transaction, RustBridgeString(scope), secret_type);
		if (!match.HasMatch()) {
			*out = RustExtString {};
			return true;
		}

		auto &kv = dynamic_cast<const KeyValueSecret &>(*match.secret_entry->secret);
		auto secret_value = kv.TryGetValue(secret_key);
		if (secret_value.IsNull()) {
			*out = RustExtString {};
			return true;
		}
		AllocateHostString(secret_value.ToString(), out);
		return true;
	} catch (std::exception &ex) {
		SetRustBridgeErrorMessage(err, ex.what());
		return false;
	}
}

static RustBridgeAttachConfig ResolveAttachConfig(ClientContext &context, AttachInfo &info,
                                                  AttachOptions &attach_options) {
	AttachHostContext host_context {context, attach_options};
	RustExtAttachHost host {HostGetAttachOption, HostLookupSecret};
	RustExtAttachConfig config {};
	RustExtError error;
	if (!rust_ext_resolve_attach(BorrowRustBridgeString(info.path), &host, &host_context, &config, &error)) {
		throw InvalidInputException("%s", TakeRustBridgeErrorMessage(error));
	}
	return RustBridgeAttachConfig(config);
}

static unique_ptr<Catalog> RustBridgeAttach(optional_ptr<StorageExtensionInfo>, ClientContext &context,
                                            AttachedDatabase &db, const string &, AttachInfo &info,
                                            AttachOptions &attach_options) {
	auto config = ResolveAttachConfig(context, info, attach_options);
	return make_uniq<RustBridgeCatalog>(db, context, std::move(config));
}

static unique_ptr<TransactionManager> RustBridgeCreateTransactionManager(optional_ptr<StorageExtensionInfo>,
                                                                         AttachedDatabase &db, Catalog &) {
	return make_uniq<RustBridgeTransactionManager>(db);
}

RustBridgeStorageExtension::RustBridgeStorageExtension() {
	attach = RustBridgeAttach;
	create_transaction_manager = RustBridgeCreateTransactionManager;
}

} // namespace duckdb
