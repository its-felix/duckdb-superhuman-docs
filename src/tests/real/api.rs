use super::*;

pub(super) struct PageCleanup {
    pub(super) endpoint: String,
    pub(super) credential: String,
    pub(super) resource: String,
    pub(super) page_id: String,
}

impl Drop for PageCleanup {
    fn drop(&mut self) {
        if let Ok(sdk) = SdkClient::at(&self.endpoint, &self.credential) {
            let resource = self.resource.clone();
            let page_id = self.page_id.clone();
            let _ = crate::platform::block_on_result(sdk.execute(|client| {
                Box::pin(async move {
                    client
                        .docs()
                        .pages()
                        .delete(operations::DeletePageInput {
                            doc_id: resource,
                            page_id_or_name: page_id,
                        })
                        .await
                })
            }));
        }
    }
}

pub(super) fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} must be set in the environment"))
}

fn api_json<T, F>(sdk: &SdkClient, operation: F) -> Result<Value, String>
where
    F: for<'a> FnOnce(&'a Client) -> crate::sdk::OperationFuture<'a, T>,
{
    json_body(crate::platform::block_on_result(sdk.execute(operation))?)
}

fn json_body(body: String) -> Result<Value, String> {
    if body.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }
}

fn paged_items<T, F>(sdk: &SdkClient, mut operation: F) -> Result<Vec<Value>, String>
where
    F: for<'a> FnMut(&'a Client, Option<String>) -> crate::sdk::OperationFuture<'a, T>,
{
    let mut out = Vec::new();
    let mut page_token = None;
    loop {
        let root = api_json(sdk, |client| operation(client, page_token.take()))?;
        if let Some(items) = root.get("items").and_then(Value::as_array) {
            out.extend(items.iter().cloned());
        }
        page_token = root
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        if page_token.is_none() {
            return Ok(out);
        }
    }
}

pub(super) fn create_test_page(
    endpoint: &str,
    credential: &str,
    resource: &str,
    page_name: &str,
    table_name: &str,
) -> Result<Value, String> {
    let html = format!(
        "<h1>{table_name}</h1>\
         <table>\
         <caption>{table_name}</caption>\
         <thead><tr><th>Name</th><th>Done</th><th>Amount</th></tr></thead>\
         <tbody>\
         <tr><td>Alpha</td><td>true</td><td>1.25</td></tr>\
         <tr><td>Beta</td><td>false</td><td>2.5</td></tr>\
         </tbody>\
         </table>"
    );
    create_page_with_html(endpoint, credential, resource, page_name, html)
}

pub(super) fn create_page_with_html(
    endpoint: &str,
    credential: &str,
    resource: &str,
    page_name: &str,
    html: String,
) -> Result<Value, String> {
    let sdk = SdkClient::at(endpoint, credential)?;
    let resource = resource.to_string();
    let page_name = page_name.to_string();
    let body = crate::platform::block_on_result(sdk.execute(|client| {
        Box::pin(async move {
            client
                .docs()
                .pages()
                .create(operations::CreatePageInput {
                    doc_id: resource,
                    payload: operations::PageCreate {
                        name: Some(page_name),
                        subtitle: None,
                        icon_name: None,
                        image_url: None,
                        parent_page_id: None,
                        page_content: Some(operations::PageCreateContent::Canvas(
                            operations::PageCreateCanvasContent {
                                type_: operations::PageType::Canvas,
                                canvas_content: operations::PageContent {
                                    format: operations::PageContentFormat::Html,
                                    content: html,
                                },
                            },
                        )),
                    },
                })
                .await
        })
    }))?;
    json_body(body)
}

pub(super) fn wait_for_page_table(
    endpoint: &str,
    credential: &str,
    resource: &str,
    page_id: &str,
    wanted_table_name: &str,
) -> Result<Value, String> {
    let sdk = SdkClient::at(endpoint, credential)?;
    for _ in 0..40 {
        let tables = paged_items(&sdk, |client, page_token| {
            let resource = resource.to_string();
            Box::pin(async move {
                client
                    .tables()
                    .list(operations::ListTablesInput {
                        doc_id: resource,
                        limit: Some(100),
                        page_token,
                        sort_by: None,
                        table_types: None,
                    })
                    .await
            })
        })?;
        for table in &tables {
            let parent_id = table
                .get("parent")
                .and_then(|parent| parent.get("id"))
                .and_then(Value::as_str);
            let table_type = table
                .get("tableType")
                .and_then(Value::as_str)
                .unwrap_or("table");
            if parent_id == Some(page_id) && table_type.eq_ignore_ascii_case("table") {
                let table_name = table.get("name").and_then(Value::as_str).unwrap_or("");
                if table_name != wanted_table_name {
                    eprintln!(
                        "Superhuman Docs named integration table '{table_name}'; requested '{wanted_table_name}'"
                    );
                }
                return Ok(table.clone());
            }
        }
        thread::sleep(Duration::from_secs(3));
    }
    Err("timed out waiting for Superhuman Docs table".to_string())
}

pub(super) fn assert_required_columns(
    endpoint: &str,
    credential: &str,
    resource: &str,
    table_id: &str,
) -> Result<(), String> {
    let sdk = SdkClient::at(endpoint, credential)?;
    let columns = paged_items(&sdk, |client, page_token| {
        let resource = resource.to_string();
        let table_id = table_id.to_string();
        Box::pin(async move {
            client
                .tables()
                .columns()
                .list(operations::ListColumnsInput {
                    doc_id: resource,
                    table_id_or_name: table_id,
                    limit: Some(100),
                    page_token,
                    visible_only: Some(false),
                })
                .await
        })
    })?;
    for required in ["Name", "Done", "Amount"] {
        let found = columns
            .iter()
            .any(|column| column.get("name").and_then(Value::as_str) == Some(required));
        if !found {
            return Err(format!("missing required column {required}"));
        }
    }
    Ok(())
}
