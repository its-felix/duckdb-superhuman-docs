#include <emscripten/fetch.h>

#include <cstdint>
#include <cstring>
#include <memory>
#include <mutex>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

using RustExtFetchCallback = void (*)(void *userdata, uint16_t status, const uint8_t *body,
                                      size_t body_len, const char *error);

namespace {

struct FetchContext {
    RustExtFetchCallback callback;
    void *userdata;
    emscripten_fetch_t *fetch = nullptr;
    std::string method;
    std::string url;
    std::vector<std::string> headers;
    std::vector<const char *> header_ptrs;
    std::vector<uint8_t> body;
    bool body_present;
};

std::mutex fetch_mutex;
std::unordered_map<uintptr_t, std::unique_ptr<FetchContext>> fetches;
uintptr_t next_fetch_id = 1;

std::unique_ptr<FetchContext> TakeFetch(uintptr_t id) {
    std::lock_guard<std::mutex> guard(fetch_mutex);
    auto entry = fetches.find(id);
    if (entry == fetches.end()) {
        return nullptr;
    }
    auto context = std::move(entry->second);
    fetches.erase(entry);
    return context;
}

void FinishFetch(emscripten_fetch_t *fetch) {
    auto id = reinterpret_cast<uintptr_t>(fetch->userData);
    auto context = TakeFetch(id);
    if (!context) {
        return;
    }
    const auto status = static_cast<uint16_t>(fetch->status);
    const auto *body = reinterpret_cast<const uint8_t *>(fetch->data);
    const char *error = status == 0 ? fetch->statusText : nullptr;
    context->callback(context->userdata, status, body, fetch->numBytes, error);
    emscripten_fetch_close(fetch);
}

} // namespace

extern "C" uintptr_t rust_ext_emscripten_fetch_start(
    const char *method, const char *url, const char *const *headers, size_t header_count,
    const uint8_t *body, size_t body_len, bool body_present, uint32_t timeout_ms,
    RustExtFetchCallback callback, void *userdata) {
    if (!method || !url || !callback) {
        return 0;
    }
    auto context = std::make_unique<FetchContext>();
    context->callback = callback;
    context->userdata = userdata;
    context->method = method;
    context->url = url;
    context->body_present = body_present;
    context->headers.reserve(header_count);
    for (size_t index = 0; index < header_count; ++index) {
        context->headers.emplace_back(headers[index]);
    }
    context->header_ptrs.reserve(header_count + 1);
    for (const auto &header : context->headers) {
        context->header_ptrs.push_back(header.c_str());
    }
    context->header_ptrs.push_back(nullptr);
    if (body_present && body_len != 0) {
        context->body.assign(body, body + body_len);
    }

    uintptr_t id;
    FetchContext *stored;
    {
        std::lock_guard<std::mutex> guard(fetch_mutex);
        id = next_fetch_id++;
        if (id == 0) {
            id = next_fetch_id++;
        }
        stored = context.get();
        fetches.emplace(id, std::move(context));
    }

    emscripten_fetch_attr_t attributes;
    emscripten_fetch_attr_init(&attributes);
    std::strncpy(attributes.requestMethod, stored->method.c_str(),
                 sizeof(attributes.requestMethod) - 1);
    attributes.requestMethod[sizeof(attributes.requestMethod) - 1] = '\0';
    attributes.attributes = EMSCRIPTEN_FETCH_LOAD_TO_MEMORY;
    attributes.timeoutMSecs = timeout_ms;
    attributes.requestHeaders = stored->header_ptrs.data();
    if (stored->body_present) {
        attributes.requestData = reinterpret_cast<const char *>(stored->body.data());
        attributes.requestDataSize = stored->body.size();
    }
    attributes.userData = reinterpret_cast<void *>(id);
    attributes.onsuccess = FinishFetch;
    attributes.onerror = FinishFetch;
    auto *fetch = emscripten_fetch(&attributes, stored->url.c_str());
    if (!fetch) {
        TakeFetch(id);
        return 0;
    }
    {
        std::lock_guard<std::mutex> guard(fetch_mutex);
        auto entry = fetches.find(id);
        if (entry != fetches.end()) {
            entry->second->fetch = fetch;
        }
    }
    return id;
}

extern "C" void rust_ext_emscripten_fetch_cancel(uintptr_t handle) {
    auto context = TakeFetch(handle);
    if (!context) {
        return;
    }
    context->callback(context->userdata, 0, nullptr, 0, "Emscripten Fetch request cancelled");
    if (context->fetch) {
        emscripten_fetch_close(context->fetch);
    }
}
