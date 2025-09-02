use futures_lite::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use trailbase_client::{
  Client, CompareOp, DbEvent, Filter, ListArguments, ListResponse, Pagination, ReadArguments,
};

struct Server {
  child: std::process::Child,
}

impl Drop for Server {
  fn drop(&mut self) {
    self.child.kill().unwrap();
  }
}

const PORT: u16 = 4057;

fn start_server() -> Result<Server, std::io::Error> {
  let cwd = std::env::current_dir()?;
  assert!(cwd.ends_with("client"));

  let command_cwd = cwd.parent().unwrap().parent().unwrap();
  let depot_path = "client/testfixture";

  let _output = std::process::Command::new("cargo")
    .args(&["build"])
    .current_dir(&command_cwd)
    .output()?;

  let args = [
    "run".to_string(),
    "--".to_string(),
    format!("--data-dir={depot_path}"),
    "run".to_string(),
    format!("--address=127.0.0.1:{PORT}"),
    "--js-runtime-threads=2".to_string(),
  ];
  let child = std::process::Command::new("cargo")
    .args(&args)
    .current_dir(&command_cwd)
    .spawn()?;

  // Wait for server to become healthy.
  let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap();

  runtime.block_on(async {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{PORT}/api/healthcheck");

    for _ in 0..100 {
      let response = client.get(&url).send().await;

      if let Ok(response) = response {
        if let Ok(body) = response.text().await {
          if body.to_uppercase() == "OK" {
            return;
          }
        }
      }

      tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    panic!("Server did not get healthy");
  });

  return Ok(Server { child });
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SimpleStrict {
  id: String,

  text_null: Option<String>,
  text_default: Option<String>,
  text_not_null: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
struct Profile {
  id: String,
  user: String,
  name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
struct Post {
  id: String,
  author: String,
  title: String,
  body: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct ProfileReference {
  id: String,
  data: Option<Profile>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct PostReference {
  id: String,
  data: Option<Post>,
}

#[allow(unused)]
#[derive(Debug, Clone, Deserialize, PartialEq)]
struct Comment {
  id: i64,
  body: String,
  author: ProfileReference,
  post: PostReference,
}

async fn connect() -> Client {
  let client = Client::new(&format!("http://127.0.0.1:{PORT}"), None).unwrap();
  let _ = client.login("admin@localhost", "secret").await.unwrap();
  return client;
}

async fn login_test() {
  let client = connect().await;

  let tokens = client.tokens().unwrap();

  assert_ne!(tokens.auth_token, "");
  assert!(tokens.refresh_token.is_some());

  let user = client.user().unwrap();
  assert_eq!(user.email, "admin@localhost");

  client.refresh().await.unwrap();

  client.logout().await.unwrap();
  assert!(client.tokens().is_none());
}

async fn records_test() {
  let client = connect().await;
  let api = client.records("simple_strict_table");

  let now = now();

  let messages = vec![
    format!("rust client test 0: =?&{now}"),
    format!("rust client test 1: =?&{now}"),
  ];

  let mut ids = vec![];
  for msg in messages.iter() {
    ids.push(api.create(json!({"text_not_null": msg})).await.unwrap());
  }

  {
    let bulk_ids = api
      .create_bulk(&[
        json!({"text_not_null": "rust bulk 0"}),
        json!({"text_not_null": "rust bulk 1"}),
      ])
      .await
      .unwrap();

    assert_eq!(2, bulk_ids.len());
  }

  {
    // List one specific message.
    let filter = Filter {
      column: "text_not_null".to_string(),
      value: messages[0].clone(),
      ..Default::default()
    };
    let response = api
      .list::<serde_json::Value>(ListArguments::new().with_filters(filter.clone()))
      .await
      .unwrap();

    assert_eq!(response.records.len(), 1, "{:?}", response.records);

    let second_response = api
      .list::<serde_json::Value>(
        ListArguments::new()
          .with_filters(filter)
          .with_pagination(Pagination::new().with_cursor(response.cursor)),
      )
      .await
      .unwrap();

    assert_eq!(second_response.records.len(), 0);
  }

  {
    // List all the messages
    let filter = Filter {
      column: "text_not_null".to_string(),
      op: Some(CompareOp::Like),
      value: format!("% =?&{now}"),
    };
    let records_ascending: ListResponse<SimpleStrict> = api
      .list(
        ListArguments::new()
          .with_order(["+text_not_null"])
          .with_filters(filter.clone())
          .with_count(true),
      )
      .await
      .unwrap();

    let messages_ascending: Vec<_> = records_ascending
      .records
      .into_iter()
      .map(|s| s.text_not_null)
      .collect();
    assert_eq!(messages, messages_ascending);
    assert_eq!(Some(2), records_ascending.total_count);

    let records_descending: ListResponse<SimpleStrict> = api
      .list(
        ListArguments::new()
          .with_order(["-text_not_null"])
          .with_filters(filter),
      )
      .await
      .unwrap();

    let messages_descending: Vec<_> = records_descending
      .records
      .into_iter()
      .map(|s| s.text_not_null)
      .collect();
    assert_eq!(
      messages,
      messages_descending.into_iter().rev().collect::<Vec<_>>()
    );
  }

  {
    // Read
    let record: SimpleStrict = api.read(&ids[0]).await.unwrap();
    assert_eq!(ids[0], record.id);
    assert_eq!(record.text_not_null, messages[0]);
  }

  {
    // Update
    let updated_message = format!("rust client updated test 0: {now}");
    api
      .update(&ids[0], json!({"text_not_null": updated_message}))
      .await
      .unwrap();

    let record: SimpleStrict = api.read(&ids[0]).await.unwrap();
    assert_eq!(record.text_not_null, updated_message);
  }

  {
    // Delete
    api.delete(&ids[0]).await.unwrap();

    let response = api.read::<SimpleStrict>(&ids[0]).await;
    assert!(response.is_err());
  }
}

async fn expand_foreign_records_test() {
  let client = connect().await;
  let api = client.records("comment");

  {
    let comment: Comment = api.read(1).await.unwrap();
    assert_eq!(1, comment.id);
    assert_eq!("first comment", comment.body);
    assert_ne!("", comment.author.id);
    assert!(comment.author.data.is_none());
    assert_ne!("", comment.post.id);
    assert!(comment.post.data.is_none());
  }

  {
    let comment: Comment = api
      .read(ReadArguments::new(1).with_expand(["post"]))
      .await
      .unwrap();
    assert_eq!(1, comment.id);
    assert_eq!("first comment", comment.body);
    assert!(comment.author.data.is_none());
    assert_eq!("first post", comment.post.data.as_ref().unwrap().title)
  }

  {
    let comments: ListResponse<Comment> = api
      .list(
        ListArguments::new()
          .with_pagination(Pagination::new().with_limit(2))
          .with_order(["-id"])
          .with_expand(["author", "post"]),
      )
      .await
      .unwrap();

    assert_eq!(2, comments.records.len());
    let first = &comments.records[0];

    assert_eq!(2, first.id);
    assert_eq!("second comment", first.body);
    assert_eq!("SecondUser", first.author.data.as_ref().unwrap().name);
    assert_eq!("first post", first.post.data.as_ref().unwrap().title);

    let second = &comments.records[0];

    let offset_comments: ListResponse<Comment> = api
      .list(
        ListArguments::new()
          .with_pagination(Pagination::new().with_limit(1).with_offset(1))
          .with_order(["-id"])
          .with_expand(["author", "post"]),
      )
      .await
      .unwrap();

    assert_eq!(1, offset_comments.records.len());
    assert_eq!(*second, offset_comments.records[0]);
  }
}

async fn subscription_test() {
  let client = connect().await;
  let api = client.records("simple_strict_table");

  let table_stream = api.subscribe("*").await.unwrap();

  let now = now();
  let create_message = format!("rust client realtime test 0: =?&{now}");
  let id = api
    .create(json!({"text_not_null": create_message}))
    .await
    .unwrap();

  let record_stream = api.subscribe(&id).await.unwrap();

  let updated_message = format!("rust client updated realtime test 0: =?&{now}");
  api
    .update(&id, json!({"text_not_null": updated_message}))
    .await
    .unwrap();

  api.delete(&id).await.unwrap();

  {
    let record_events = record_stream.collect::<Vec<_>>().await;
    match &record_events[0] {
      DbEvent::Update(Some(serde_json::Value::Object(obj))) => {
        assert_eq!(obj["text_not_null"], updated_message);
      }
      msg => panic!("Unexpected event: {msg:?}"),
    };
    match &record_events[1] {
      DbEvent::Delete(Some(serde_json::Value::Object(obj))) => {
        assert_eq!(obj["text_not_null"], updated_message);
      }
      msg => panic!("Unexpected event: {msg:?}"),
    };
  }

  {
    let table_events = table_stream.take(3).collect::<Vec<_>>().await;
    match &table_events[0] {
      DbEvent::Insert(Some(serde_json::Value::Object(obj))) => {
        assert_eq!(obj["text_not_null"], create_message);
      }
      msg => panic!("Unexpected event: {msg:?}"),
    };
    match &table_events[1] {
      DbEvent::Update(Some(serde_json::Value::Object(obj))) => {
        assert_eq!(obj["text_not_null"], updated_message);
      }
      msg => panic!("Unexpected event: {msg:?}"),
    };
    match &table_events[2] {
      DbEvent::Delete(Some(serde_json::Value::Object(obj))) => {
        assert_eq!(obj["text_not_null"], updated_message);
      }
      msg => panic!("Unexpected event: {msg:?}"),
    };
  }
}

#[test]
fn integration_test() {
  let _server = start_server().unwrap();

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

  runtime.block_on(login_test());
  println!("Ran login tests");

  runtime.block_on(records_test());
  println!("Ran records tests");

  runtime.block_on(expand_foreign_records_test());
  println!("Ran expand foreign records tests");

  runtime.block_on(subscription_test());
  println!("Ran subscription tests");
}

fn now() -> u64 {
  return std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .expect("Duration since epoch")
    .as_secs();
}
