use superhuman_docs_async::operations;

use crate::ffi::{RustExtScanBatch, RustExtScanRequest};
use crate::json::ffi::ffi_scan_batch;
use crate::json::rows_from_json;
use crate::model::{
    SuperhumanDocsClientConfig, SuperhumanDocsRowsRequest, SuperhumanDocsRowsResponse,
    SuperhumanDocsTable,
};
use crate::sdk::{non_empty_string, SdkClient};

async fn list_rows(
    sdk: &SdkClient,
    doc_id: &str,
    table_id: &str,
    request: SuperhumanDocsRowsRequest,
) -> Result<SuperhumanDocsRowsResponse, String> {
    let sort_by = rows_sort_by(&request.sort_by)?;
    let doc_id = doc_id.to_string();
    let table_id = table_id.to_string();
    let body = sdk
        .execute(|client| {
            Box::pin(async move {
                client
                    .tables()
                    .rows()
                    .list(operations::ListRowsInput {
                        doc_id,
                        table_id_or_name: table_id,
                        query: non_empty_string(&request.query),
                        sort_by,
                        use_column_names: Some(false),
                        value_format: Some(operations::ValueFormat::Rich),
                        visible_only: Some(false),
                        limit: Some(request.limit as i32),
                        page_token: non_empty_string(&request.page_token),
                        sync_token: non_empty_string(&request.sync_token),
                    })
                    .await
            })
        })
        .await?;
    rows_from_json(&body)
}

fn rows_sort_by(value: &str) -> Result<Option<operations::RowsSortBy>, String> {
    match value {
        "" => Ok(None),
        "createdAt" => Ok(Some(operations::RowsSortBy::CreatedAt)),
        "natural" => Ok(Some(operations::RowsSortBy::Natural)),
        "updatedAt" => Ok(Some(operations::RowsSortBy::UpdatedAt)),
        _ => Err(format!("unsupported row sort order: {value}")),
    }
}

pub(crate) struct ScanHandle {
    sdk: SdkClient,
    doc_id: String,
    table_id: String,
    query: String,
    sort_by: String,
    limit: u64,
    next_page_token: String,
    next_sync_token: String,
    sync_check_done: bool,
    finished: bool,
}

impl ScanHandle {
    pub(crate) fn new(
        config: &SuperhumanDocsClientConfig,
        table: &SuperhumanDocsTable,
        request: RustExtScanRequest,
    ) -> Result<Self, String> {
        Ok(Self {
            sdk: SdkClient::new(config)?,
            doc_id: config.resource.clone(),
            table_id: table.id.clone(),
            query: request.filter.as_str().to_string(),
            sort_by: request.order.as_str().to_string(),
            limit: if request.limit == 0 {
                500
            } else {
                request.limit.min(500)
            },
            next_page_token: String::new(),
            next_sync_token: String::new(),
            sync_check_done: false,
            finished: false,
        })
    }

    fn request(&mut self) -> SuperhumanDocsRowsRequest {
        let sync_token = if self.next_page_token.is_empty()
            && !self.next_sync_token.is_empty()
            && !self.sync_check_done
        {
            self.sync_check_done = true;
            self.next_sync_token.clone()
        } else {
            String::new()
        };
        SuperhumanDocsRowsRequest {
            page_token: self.next_page_token.clone(),
            query: self.query.clone(),
            sort_by: self.sort_by.clone(),
            sync_token,
            limit: self.limit,
        }
    }

    pub(crate) async fn next_batch(&mut self) -> Result<RustExtScanBatch, String> {
        while !self.finished {
            let request = self.request();
            let response = list_rows(&self.sdk, &self.doc_id, &self.table_id, request).await?;
            self.next_page_token = response.next_page_token;
            self.next_sync_token = response.next_sync_token;
            if self.next_page_token.is_empty()
                && (self.next_sync_token.is_empty() || self.sync_check_done)
            {
                self.finished = true;
            }

            let visible_rows = response
                .rows
                .into_iter()
                .filter(|row| !row.deleted)
                .collect::<Vec<_>>();
            if !visible_rows.is_empty() || self.finished {
                return Ok(ffi_scan_batch(visible_rows, self.finished));
            }
        }
        Ok(ffi_scan_batch(Vec::new(), true))
    }
}
