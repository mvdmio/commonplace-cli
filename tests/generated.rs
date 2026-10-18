mod support;

use std::fs::{self, File};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use support::{
    DESCRIPTION_PATH, FakeServer, Recorded, Reply, Run, TOKEN, body_json, commonplace_in, fixture,
};
use tempfile::TempDir;

const FILE_BYTES: &[u8] = b"\x00\x01binary\xff";

/// A fake Commonplace host that serves an API description which a test can change between runs, and one cache
/// folder shared by every run of the test.
struct Host {
    server: FakeServer,
    description: Arc<Mutex<Option<Value>>>,
    cache: TempDir,
}

impl Host {
    fn start() -> Self {
        let description = Arc::new(Mutex::new(Some(fixture())));
        let served = Arc::clone(&description);
        let server = FakeServer::start(move |request| {
            if request.path() == DESCRIPTION_PATH {
                return match served.lock().unwrap().as_ref() {
                    Some(document) => Reply::json(200, &document.to_string()),
                    None => Reply::empty(503),
                };
            }
            if request.path().starts_with("/api/v1/images/") {
                return Reply::with_type(200, "application/octet-stream", FILE_BYTES)
                    .header("Content-Disposition", "attachment; filename=\"cover.png\"");
            }
            Reply::json(200, "{\"done\":true}")
        });
        Host {
            server,
            description,
            cache: tempfile::tempdir().expect("a cache folder"),
        }
    }

    fn serve(&self, document: Option<Value>) {
        *self.description.lock().unwrap() = document;
    }

    fn run(&self, args: &[&str]) -> Run {
        self.run_in(&std::env::temp_dir(), args)
    }

    fn run_in(&self, dir: &std::path::Path, args: &[&str]) -> Run {
        let url = self.server.url();
        let cache = self.cache.path().to_str().expect("a UTF-8 path");
        commonplace_in(
            dir,
            args,
            &[
                ("COMMONPLACE_URL", &url),
                ("COMMONPLACE_TOKEN", TOKEN),
                ("COMMONPLACE_CACHE_DIR", cache),
            ],
        )
    }

    fn description_fetches(&self) -> usize {
        self.server
            .requests()
            .iter()
            .filter(|request| request.path() == DESCRIPTION_PATH)
            .count()
    }

    fn api_requests(&self) -> Vec<Recorded> {
        self.server
            .requests()
            .into_iter()
            .filter(|request| request.path() != DESCRIPTION_PATH)
            .collect()
    }

    fn only_api_request(&self) -> Recorded {
        let requests = self.api_requests();
        assert_eq!(requests.len(), 1, "requests: {requests:#?}");
        requests.into_iter().next().unwrap()
    }

    /// Makes every cached copy look `age` old.
    fn age_cache(&self, age: Duration) {
        for entry in fs::read_dir(self.cache.path()).expect("the cache folder") {
            let file = File::options()
                .write(true)
                .open(entry.expect("a cache entry").path())
                .expect("open the cached copy");
            file.set_modified(SystemTime::now() - age)
                .expect("age the cached copy");
        }
    }
}

const IMAGE_KEY: &str = "3f2b8c1e-5a4d-4e6f-9b7a-0c1d2e3f4a5b";

/// The fixture with one more operation, `POST <path>`, that has the given path parameters in the given order
/// and no body.
fn with_operation_taking(group_action: &str, path: &str, path_parameters: &[&str]) -> Value {
    let mut document = fixture();
    let parameters: Vec<Value> = path_parameters
        .iter()
        .map(|name| json!({ "name": name, "in": "path", "required": true, "schema": { "type": "string" } }))
        .collect();
    document["paths"][path] = json!({
        "post": {
            "tags": ["Books"],
            "summary": "A new operation",
            "operationId": group_action,
            "parameters": parameters,
            "responses": { "200": { "description": "OK", "content": { "application/json": {} } } }
        }
    });
    document
}

fn with_operation(group_action: &str, path: &str) -> Value {
    with_operation_taking(group_action, path, &["id"])
}

#[test]
fn an_action_with_a_path_parameter_sends_its_method_path_and_body() {
    let host = Host::start();

    let run = host.run(&["books", "update", "7", "--is-reading", "true"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "done": true }));
    let request = host.only_api_request();
    assert_eq!(request.method, "PATCH");
    assert_eq!(request.url, "/api/v1/books/7");
    assert_eq!(request.header("Authorization"), Some("Bearer cmp_pat_test"));
    assert_eq!(body_json(&request), json!({ "isReading": true }));
}

#[test]
fn an_action_with_no_body_sends_none() {
    let host = Host::start();

    let run = host.run(&["legal-acceptance", "accept"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "/api/v1/legal-acceptance");
    assert!(request.body.is_empty());
}

#[test]
fn path_parameters_follow_the_path_order_and_are_encoded() {
    let host = Host::start();
    // The parameters are declared in the other order than the path names them.
    host.serve(Some(with_operation_taking(
        "books.pin-quote",
        "/api/v1/books/{bookId}/quotes/{quoteId}/pin",
        &["quoteId", "bookId"],
    )));

    let run = host.run(&["books", "pin-quote", "my shelf", "page 3/4"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        host.only_api_request().url,
        "/api/v1/books/my%20shelf/quotes/page%203%2F4/pin"
    );
}

#[test]
fn a_number_path_parameter_refuses_text_before_any_request() {
    let host = Host::start();

    let run = host.run(&["books", "update", "seven", "--title", "Overstory"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn odata_options_send_their_dollar_names() {
    let host = Host::start();

    let run = host.run(&[
        "books",
        "list",
        "--top",
        "2",
        "--skip",
        "4",
        "--filter",
        "isReading eq true",
        "--select",
        "title,author",
        "--orderby",
        "title desc",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), "/api/v1/books");
    assert_eq!(request.form("$top").as_deref(), Some("2"));
    assert_eq!(request.form("$skip").as_deref(), Some("4"));
    assert_eq!(
        request.form("$filter").as_deref(),
        Some("isReading eq true")
    );
    assert_eq!(request.form("$select").as_deref(), Some("title,author"));
    assert_eq!(request.form("$orderby").as_deref(), Some("title desc"));
    assert!(request.url.contains("$top=2"), "{}", request.url);
}

#[test]
fn an_integer_option_refuses_text_before_any_request() {
    let host = Host::start();

    let run = host.run(&["books", "list", "--top", "two"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn body_options_are_typed_from_the_schema() {
    let host = Host::start();

    let book = host.run(&[
        "books",
        "create",
        "--title",
        "The Overstory",
        "--author",
        "Richard Powers",
        "--is-reading",
        "true",
        "--is-favorite",
        "false",
    ]);
    let note = host.run(&[
        "notes",
        "create",
        "--notebook-id",
        "3",
        "--title",
        "Trees",
        "--body-html",
        "<p>Roots talk.</p>",
    ]);

    assert_eq!(book.code, 0, "{book:#?}");
    assert_eq!(note.code, 0, "{note:#?}");
    let requests = host.api_requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].url, "/api/v1/books");
    assert_eq!(requests[0].header("Content-Type"), Some("application/json"));
    assert_eq!(
        body_json(&requests[0]),
        json!({
            "title": "The Overstory",
            "author": "Richard Powers",
            "isReading": true,
            "isFavorite": false,
        })
    );
    assert_eq!(
        body_json(&requests[1]),
        json!({ "notebookId": 3, "title": "Trees", "bodyHtml": "<p>Roots talk.</p>" })
    );
}

#[test]
fn array_and_object_body_options_send_json_arrays_and_objects() {
    let host = Host::start();

    let run = host.run(&[
        "conversations",
        "send-message",
        "c1",
        "--text",
        "Summarise this note",
        "--upload-ids",
        "u1",
        "--upload-ids",
        "u2",
        "--page-context",
        "{\"type\":\"note\",\"id\":\"12\"}",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.url, "/api/v1/conversations/c1/messages");
    assert_eq!(
        body_json(&request),
        json!({
            "text": "Summarise this note",
            "uploadIds": ["u1", "u2"],
            "pageContext": { "type": "note", "id": "12" },
        })
    );
}

#[test]
fn field_options_win_over_the_same_keys_in_body() {
    let host = Host::start();

    let run = host.run(&[
        "books",
        "create",
        "--body",
        "{\"title\":\"Old\",\"author\":\"Richard Powers\"}",
        "--title",
        "New",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        body_json(&host.only_api_request()),
        json!({ "title": "New", "author": "Richard Powers" })
    );
}

#[test]
fn body_reads_a_file() {
    let host = Host::start();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("book.json"), "{\"title\":\"From a file\"}").unwrap();

    let run = host.run_in(dir.path(), &["books", "create", "--body", "@book.json"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.header("Content-Type"), Some("application/json"));
    assert_eq!(body_json(&request), json!({ "title": "From a file" }));
}

#[test]
fn a_required_body_with_no_input_is_an_empty_object_and_an_optional_one_is_left_out() {
    let host = Host::start();

    let required = host.run(&["books", "create"]);
    let optional = host.run(&["conversations", "create"]);

    assert_eq!(required.code, 0, "{required:#?}");
    assert_eq!(optional.code, 0, "{optional:#?}");
    let requests = host.api_requests();
    assert_eq!(requests[0].body, b"{}");
    assert_eq!(requests[1].url, "/api/v1/conversations");
    assert!(requests[1].body.is_empty());
    assert_eq!(requests[1].header("Content-Type"), None);
}

#[test]
fn a_clash_renames_the_later_option_and_help_says_so() {
    // No Commonplace operation has a query parameter and a body field of one name, so this one is added.
    let mut document = fixture();
    document["paths"]["/api/v1/books/{id}/archive"] = json!({
        "post": {
            "tags": ["Books"],
            "operationId": "books.archive",
            "parameters": [
                { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } },
                { "name": "notify", "in": "query", "schema": { "type": "boolean" } }
            ],
            "requestBody": {
                "content": { "application/json": { "schema": { "type": "object", "properties": {
                    "notify": { "type": ["null", "boolean"] },
                    "out": { "type": ["null", "string"] }
                } } } }
            },
            "responses": { "200": { "description": "OK", "content": { "application/json": {} } } }
        }
    });
    let host = Host::start();
    host.serve(Some(document));

    let run = host.run(&[
        "books",
        "archive",
        "b1",
        "--notify",
        "true",
        "--field-notify",
        "false",
        "--field-out",
        "Shelved",
    ]);
    let help = host.run(&["books", "archive", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.api_requests().remove(0);
    assert_eq!(request.path(), "/api/v1/books/b1/archive");
    assert_eq!(request.form("notify").as_deref(), Some("true"));
    assert_eq!(
        body_json(&request),
        json!({ "notify": false, "out": "Shelved" })
    );
    assert!(
        help.stdout
            .contains("named --field-notify because --notify is taken"),
        "{}",
        help.stdout
    );
    assert!(
        help.stdout
            .contains("named --field-out because --out is taken"),
        "{}",
        help.stdout
    );
}

#[test]
fn help_on_an_action_shows_its_summary_description_and_parameters() {
    let host = Host::start();

    let update = host.run(&["books", "update", "--help"]);
    let create = host.run(&["books", "create", "--help"]);
    let list = host.run(&["books", "list", "--help"]);

    assert_eq!(update.code, 0, "{update:#?}");
    for expected in [
        "Change a Book",
        "Changes the fields you send",
        "Type: integer.",
    ] {
        assert!(
            update.stdout.contains(expected),
            "{expected}: {}",
            update.stdout
        );
    }
    for expected in [
        "Create a Book",
        "Google Books is never asked",
        "--title <STRING>",
        "--is-reading <BOOLEAN>",
        "--body <JSON|@FILE>",
    ] {
        assert!(
            create.stdout.contains(expected),
            "{expected}: {}",
            create.stdout
        );
    }
    for expected in [
        "List Books",
        "--filter <STRING>",
        "--top <INTEGER>",
        "--skip <INTEGER>",
    ] {
        assert!(
            list.stdout.contains(expected),
            "{expected}: {}",
            list.stdout
        );
    }
    assert!(host.api_requests().is_empty());
}

#[test]
fn help_on_a_group_lists_its_actions_and_its_tags() {
    let host = Host::start();

    let run = host.run(&["conversations", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    for expected in [
        "create",
        "Start a conversation",
        "send-message",
        "Send a message to the Assistant",
        "list-messages",
        "Read messages",
        "Tags: Conversations.",
    ] {
        assert!(run.stdout.contains(expected), "{expected}: {}", run.stdout);
    }
}

#[test]
fn help_lists_the_hand_written_commands_and_the_generated_groups() {
    let host = Host::start();

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    for expected in [
        "login",
        "logout",
        "status",
        "chat",
        "api",
        "skill",
        "accounts",
        "books",
        "notes",
        "images",
        "conversations",
        "uploads",
        "legal-acceptance",
    ] {
        assert!(run.stdout.contains(expected), "{expected}: {}", run.stdout);
    }
}

#[test]
fn a_second_run_within_the_hour_uses_the_cached_copy() {
    let host = Host::start();

    host.run(&["books", "list"]);
    let second = host.run(&["books", "list"]);

    assert_eq!(second.code, 0, "{second:#?}");
    assert_eq!(host.description_fetches(), 1);
}

#[test]
fn a_copy_older_than_an_hour_is_fetched_again() {
    let host = Host::start();
    host.run(&["books", "list"]);
    host.age_cache(Duration::from_secs(61 * 60));

    let run = host.run(&["books", "list"]);
    let after = host.run(&["books", "list"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(after.code, 0, "{after:#?}");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_stale_copy_is_used_when_the_fetch_fails() {
    let host = Host::start();
    host.run(&["books", "list", "--top", "1"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["books", "list", "--top", "2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/books?$top=2");
}

#[test]
fn no_copy_and_no_fetch_is_a_network_error() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["books", "list"]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "network");
    assert!(host.api_requests().is_empty());
}

#[test]
fn hand_written_commands_never_fetch_the_description() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["api", "GET", "/api/v1/books"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.description_fetches(), 0);
}

#[test]
fn an_operation_added_after_caching_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["books", "list"]);
    host.serve(Some(with_operation(
        "books.archive",
        "/api/v1/books/{id}/archive",
    )));

    let run = host.run(&["books", "archive", "1"]);
    let again = host.run(&["books", "archive", "2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(again.code, 0, "{again:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/books/1/archive");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_new_group_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["books", "list"]);
    host.serve(Some(with_operation(
        "quotes.pin",
        "/api/v1/quotes/{id}/pin",
    )));

    let run = host.run(&["quotes", "pin", "q1"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/quotes/q1/pin");
}

#[test]
fn a_command_still_unknown_after_the_refetch_is_a_usage_mistake() {
    let host = Host::start();
    host.run(&["books", "list"]);

    let action = host.run(&["books", "explode", "1"]);
    let group = host.run(&["dragons", "list"]);

    assert_eq!(action.code, 2, "{action:#?}");
    assert_eq!(action.stderr_json()["error"], "usage");
    assert_eq!(group.code, 2, "{group:#?}");
    assert_eq!(host.description_fetches(), 3);
    assert_eq!(host.api_requests().len(), 1);
}

#[test]
fn the_hand_written_accounts_list_wins_over_the_generated_one() {
    // Commonplace's accounts.list takes no options, so this one is given `$top`, and the group gets one more
    // operation the hand-written commands do not have.
    let mut document = with_operation_taking(
        "accounts.get",
        "/api/v1/accounts/{accountId}",
        &["accountId"],
    );
    document["paths"]["/api/v1/accounts"]["get"]["parameters"] = json!([
        { "name": "$top", "in": "query", "schema": { "type": ["integer", "string"] } }
    ]);
    document["paths"]["/api/v1/accounts/{accountId}"]["post"]["summary"] = json!("Get an Account");
    let host = Host::start();
    host.serve(Some(document));

    // The generated accounts.list takes --top; the hand-written one does not.
    let hand_written = host.run(&["accounts", "list", "--top", "1"]);
    let generated = host.run(&["accounts", "get", "7"]);
    let help = host.run(&["accounts", "--help"]);

    assert_eq!(hand_written.code, 2, "{hand_written:#?}");
    assert_eq!(generated.code, 0, "{generated:#?}");
    assert_eq!(host.only_api_request().url, "/api/v1/accounts/7");
    for expected in ["list", "switch", "get", "Get an Account"] {
        assert!(
            help.stdout.contains(expected),
            "{expected}: {}",
            help.stdout
        );
    }
}

#[test]
fn camel_case_and_acronym_names_become_kebab_case_commands() {
    let host = Host::start();
    host.serve(Some(with_operation_taking(
        "URLPath.getAllItems",
        "/api/v1/url-paths",
        &[],
    )));

    let run = host.run(&["url-path", "get-all-items"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.only_api_request().url, "/api/v1/url-paths");
}

#[test]
fn the_first_of_two_operations_with_the_same_name_wins() {
    let host = Host::start();
    let mut document = with_operation_taking("reports.run", "/api/v1/reports-a", &[]);
    document["paths"]["/api/v1/reports-b"] = document["paths"]["/api/v1/reports-a"].clone();
    document["paths"]["/api/v1/reports-b"]["post"]["summary"] = json!("The duplicate");
    host.serve(Some(document));

    let run = host.run(&["reports", "run"]);
    let help = host.run(&["reports", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.only_api_request().url, "/api/v1/reports-a");
    assert!(!help.stdout.contains("The duplicate"), "{}", help.stdout);
}

#[test]
fn an_action_or_group_named_help_is_hidden() {
    let host = Host::start();
    let mut document = with_operation_taking("books.help", "/api/v1/books-help", &[]);
    document["paths"]["/api/v1/books-help"]["post"]["summary"] = json!("Hidden action");
    document["paths"]["/api/v1/help"] = json!({
        "post": {
            "summary": "Hidden group",
            "operationId": "help.list",
            "responses": { "200": { "description": "OK" } }
        }
    });
    host.serve(Some(document));

    let action = host.run(&["books", "help"]);
    let group = host.run(&["help", "list"]);
    let books = host.run(&["books", "--help"]);
    let top = host.run(&["--help"]);

    assert_eq!(action.code, 2, "{action:#?}");
    assert_eq!(group.code, 2, "{group:#?}");
    assert!(!books.stdout.contains("Hidden action"), "{}", books.stdout);
    assert!(!top.stdout.contains("Hidden group"), "{}", top.stdout);
    assert!(host.api_requests().is_empty());
}

#[test]
fn the_hand_written_status_hides_the_generated_status_get() {
    let host = Host::start();
    let mut document = with_operation_taking("status.get", "/api/v1/status", &[]);
    document["paths"]["/api/v1/status"]["post"]["summary"] = json!("Hidden status");
    host.serve(Some(document));

    let run = host.run(&["status", "get"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert!(host.api_requests().is_empty());
}

#[test]
fn a_download_writes_the_bytes_to_out_and_prints_a_summary() {
    let host = Host::start();
    let dir = tempfile::tempdir().unwrap();

    let run = host.run_in(
        dir.path(),
        &["images", "get", IMAGE_KEY, "--out", "photo.png"],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(fs::read(dir.path().join("photo.png")).unwrap(), FILE_BYTES);
    assert_eq!(
        run.stdout_json(),
        json!({
            "path": "photo.png",
            "bytes": FILE_BYTES.len(),
            "contentType": "application/octet-stream",
            "fileName": "cover.png",
        })
    );
    assert_eq!(
        host.only_api_request().url,
        format!("/api/v1/images/{IMAGE_KEY}")
    );
}

#[test]
fn a_download_without_out_is_a_usage_mistake_and_sends_no_request() {
    let host = Host::start();

    let run = host.run(&["images", "get", IMAGE_KEY]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn out_on_a_command_that_answers_json_is_a_usage_mistake() {
    let host = Host::start();

    let run = host.run(&["books", "list", "--out", "books.json"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn an_upload_target_field_says_so_in_help() {
    let host = Host::start();

    let run = host.run(&["conversations", "send-message", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(
        run.stdout.contains("--upload-ids <STRING>"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("upload target `conversation`"),
        "{}",
        run.stdout
    );
}

#[test]
fn only_an_operation_with_an_upload_target_takes_file() {
    let host = Host::start();

    let book = host.run(&["books", "create", "--help"]);
    let note = host.run(&["notes", "create", "--help"]);
    let refused = host.run(&["books", "create", "--file", "cover.png"]);

    assert_eq!(book.code, 0, "{book:#?}");
    assert!(!book.stdout.contains("--file"), "{}", book.stdout);
    assert!(!note.stdout.contains("--file"), "{}", note.stdout);
    assert_eq!(refused.code, 2, "{refused:#?}");
    assert_eq!(refused.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

fn answering(status: u16, problem: &'static str) -> FakeServer {
    FakeServer::start(move |request| {
        if request.path() == DESCRIPTION_PATH {
            return Reply::json(200, &fixture().to_string());
        }
        Reply::problem(status, problem)
    })
}

fn run_against(server: &FakeServer, args: &[&str]) -> Run {
    commonplace_in(
        &std::env::temp_dir(),
        args,
        &[
            ("COMMONPLACE_URL", &server.url()),
            ("COMMONPLACE_TOKEN", TOKEN),
        ],
    )
}

#[test]
fn legal_acceptance_hands_off_to_the_browser_with_exit_3() {
    let server = answering(
        403,
        "{\"type\":\"browser-handoff\",\"title\":\"Accept the terms\",\"status\":403,\"detail\":\"Accept the Legal terms in the browser.\",\"handoffUrl\":\"https://commonplace.example/Legal/Accept\"}",
    );

    let run = run_against(&server, &["legal-acceptance", "accept"]);

    assert_eq!(run.code, 3, "{run:#?}");
    assert_eq!(
        run.stdout_json()["handoffUrl"],
        "https://commonplace.example/Legal/Accept"
    );
    assert_eq!(run.stdout_json()["status"], "browser_handoff");
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|request| request.path() != DESCRIPTION_PATH)
            .count(),
        1
    );
}

#[test]
fn a_browser_handoff_on_a_books_call_keeps_the_exit_code() {
    let server = answering(
        403,
        "{\"type\":\"browser-handoff\",\"title\":\"Accept the terms\",\"status\":403,\"handoffUrl\":\"https://commonplace.example/Legal/Accept\"}",
    );

    let run = run_against(&server, &["books", "list"]);

    assert_eq!(run.code, 3, "{run:#?}");
    assert_eq!(
        run.stdout_json()["handoffUrl"],
        "https://commonplace.example/Legal/Accept"
    );
}

#[test]
fn an_api_problem_is_printed_unchanged_and_exits_1() {
    const PROBLEM: &str = "{\"type\":\"validation\",\"title\":\"One or more fields are invalid.\",\"status\":422,\"errors\":{\"title\":[\"Title is required.\"]}}";
    let server = answering(422, PROBLEM);

    let run = run_against(&server, &["books", "create", "--author", "Richard Powers"]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr.trim_end(), PROBLEM);
    assert!(run.stdout.is_empty());
}

#[test]
fn help_without_the_description_lists_the_hand_written_commands_and_says_what_is_missing() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(run.stdout.contains("login"), "{}", run.stdout);
    assert!(!run.stdout.contains("legal-acceptance"), "{}", run.stdout);
    assert!(
        run.stdout.contains("openapi/v1.json are missing"),
        "{}",
        run.stdout
    );
    assert_ends_with_the_docs_address(&host, &run);
}

fn assert_ends_with_the_docs_address(host: &Host, run: &Run) {
    let expected = format!("Docs: {}/agents", host.server.url());
    assert!(run.stdout.trim_end().ends_with(&expected), "{}", run.stdout);
}

#[test]
fn help_ends_with_the_docs_address_on_the_current_host() {
    let host = Host::start();

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_ends_with_the_docs_address(&host, &run);
}

#[test]
fn an_unknown_command_with_a_failed_fetch_does_not_fetch_twice() {
    let host = Host::start();
    host.run(&["books", "list"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["books", "explode", "1"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(host.description_fetches(), 2);
}
