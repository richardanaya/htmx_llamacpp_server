use askama::Template;
use axum::extract::{FromRef, State};
use axum::http::StatusCode;
use axum::response::Redirect;
use axum::response::{Html, Result};
use axum::{
    routing::{delete, get, post},
    Router,
};
use axum_extra::extract::cookie::{Cookie, Key, PrivateCookieJar};
use axum_extra::extract::Form;
use axum_extra::response::Css;
use clap::Parser;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone)]
struct AppState {
    key: Key,
    url: String,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.key.clone()
    }
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(long = "llama")]
    llamma_cpp_server: String,

    // Port to listen on
    #[arg(long, default_value = "3000")]
    port: u16,

    // Host to listen on
    #[arg(default_value = "127.0.0.1")]
    host: String,
}

#[derive(Deserialize, Serialize, Clone)]
struct LoginParams {
    username: String,
}

#[derive(Deserialize, Serialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize, Serialize)]
struct ClearMessagesPayload {
    context: String,
}

#[derive(Deserialize, Serialize)]
struct SendMessageRequest {
    messages: Option<Vec<String>>,
    roles: Option<Vec<String>>,
    context: String,
    user_message: String,
}

#[derive(Serialize, Deserialize)]
struct Choice {
    index: f64,
    message: ChatMessage,
    logprobs: Option<String>,
    finish_reason: String,
}

#[derive(Serialize, Deserialize)]
struct LlamaResponse {
    created: f64,
    choices: Vec<Choice>,
}

#[derive(Serialize, Deserialize)]
struct LlamaRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

#[derive(Template)]
#[template(path = "chat.jinja")]
struct ChatFragmentTemplate {
    messages: Vec<ChatMessage>,
    context: String,
    user_message: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct ChatTemplate {
    messages: Vec<ChatMessage>,
    context: String,
    user_message: String,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {}

fn render_template(template: impl Template) -> Result<Html<String>, StatusCode> {
    let result = template.render();
    match result {
        Ok(x) => Ok(Html(x)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn logout(jar: PrivateCookieJar) -> (PrivateCookieJar, Redirect) {
    (jar.remove(Cookie::build("name")), Redirect::to("/"))
}

async fn login(
    jar: PrivateCookieJar,
    Form(params): Form<LoginParams>,
) -> (PrivateCookieJar, Redirect) {
    let updated_jar = jar.add(Cookie::new("name", params.username));
    (updated_jar, Redirect::to("/"))
}

async fn index(jar: PrivateCookieJar) -> Result<Html<String>, StatusCode> {
    let name = jar.get("name");

    if let Some(name) = name {
        render_template(ChatTemplate {
            messages: vec![ChatMessage {
                role: "AI".to_string(),
                content: format!(
                    "Hello {}, welcome to a demo of HTMX and Llama.cpp server",
                    name.value().to_string()
                )
                .to_string(),
            }],
            context: "".to_string(),
            user_message: "".to_string(),
        })
    } else {
        render_template(LoginTemplate {})
    }
}

async fn send_message(
    jar: PrivateCookieJar,
    State(state): State<AppState>,
    Form(form): Form<SendMessageRequest>,
) -> Result<Html<String>, StatusCode> {
    let name = match jar.get("name") {
        Some(x) => x,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let all_roles = form.roles.clone().unwrap_or(vec![]);
    let all_messages = form.messages.clone().unwrap_or(vec![]);

    let mut chat_messages: Vec<ChatMessage> = all_messages
        .iter()
        .enumerate()
        .map(|(i, x)| ChatMessage {
            role: all_roles[i].clone(),
            content: x.clone(),
        })
        .collect();

    chat_messages.push(ChatMessage {
        role: name.value().to_string(),
        content: form.user_message.clone(),
    });

    let client = reqwest::Client::new();

    let mut chat_messages_with_system_context = chat_messages.clone();

    // add context to front
    chat_messages_with_system_context.insert(
        0,
        ChatMessage {
            role: "System".to_string(),
            content: form.context.clone(),
        },
    );

    let data: LlamaRequest = LlamaRequest {
        model: "gpt-3.5-turbo".to_string(),
        messages: chat_messages_with_system_context.clone(),
    };

    let response = match client.post(state.url).json(&data).send().await {
        Ok(x) => x,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let body = match response.json::<LlamaResponse>().await {
        Ok(x) => x,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let response_message = body.choices[0].message.content.clone();

    // TODO: ai generation
    chat_messages.push(ChatMessage {
        role: "AI".to_string(),
        content: response_message,
    });

    render_template(ChatFragmentTemplate {
        messages: chat_messages,
        context: form.context.clone(),
        user_message: "".to_string(),
    })
}

async fn clear_messages(
    Form(payload): Form<ClearMessagesPayload>,
) -> Result<Html<String>, StatusCode> {
    render_template(ChatFragmentTemplate {
        messages: vec![],
        context: payload.context.clone(),
        user_message: "".to_string(),
    })
}

async fn get_style() -> Css<String> {
    Css(include_str!("../static/style.css").to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // build our application with a single route
    let app = Router::new()
        .route("/", get(index).merge(post(login)).merge(delete(logout)))
        .route("/send_message", post(send_message))
        .route("/clear_messages", post(clear_messages))
        .route("/style.css", get(get_style))
        .with_state(AppState {
            key: Key::generate(),
            url: format!("{}/v1/chat/completions", args.llamma_cpp_server).to_string(),
        });

    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Listening on {}", listener.local_addr()?);
    Ok(axum::serve(listener, app).await?)
}
