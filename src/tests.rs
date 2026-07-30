use crate::attach::{
    doc_id_from_browser_url, doc_id_from_resolved_link, is_browser_url, read_environment_variable,
    resolve_attach, strip_attach_resource_prefix,
};
use crate::ffi::*;
use crate::json::{column_list_from_json, logical_type, logical_type_alias, rows_from_json};
use crate::model::{
    SuperhumanDocsCell, SuperhumanDocsClientConfig, SuperhumanDocsColumn, SuperhumanDocsRow,
};
use crate::mutation::{build_equality_query, insert_payload, update_payload};
use crate::scan::scan_value;
use crate::sdk::{validate_token_at, SdkClient};
use crate::secret::{create_secret, free_secret};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::ffi::{c_char, c_void, CStr};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use superhuman_docs_async::{operations, Client, DEFAULT_BASE_URL};

static NETWORK_UNIT_TEST_LOCK: Mutex<()> = Mutex::new(());

mod duckdb;
mod mock;
mod mock_server;
mod real;
mod unit;

use duckdb::*;
use mock_server::MockSuperhumanDocsServer;
