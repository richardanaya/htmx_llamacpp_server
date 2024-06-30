use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, Result};
use axum::{
    routing::{get, post},
    Router,
};
use axum_extra::extract::Form;
use clap::Parser;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone)]
struct AppState {
    url: String,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long)]
    llamma_cpp_server: String,
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
    messages: Vec<String>,
    roles: Vec<String>,
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
#[template(path = "chat.html")]
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

fn render_template(template: impl Template) -> Result<Html<String>, StatusCode> {
    let result = template.render();
    match result {
        Ok(x) => Ok(Html(x)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn index() -> Result<Html<String>, StatusCode> {
    render_template(ChatTemplate {
        messages: vec![ChatMessage {
            role: "AI".to_string(),
            content: "Hello, I am a bot".to_string(),
        }],
        context: "".to_string(),
        user_message: "".to_string(),
    })
}

async fn send_message(
    State(state): State<AppState>,
    Form(form): Form<SendMessageRequest>,
) -> Result<Html<String>, StatusCode> {
    let mut chat_messages: Vec<ChatMessage> = form
        .messages
        .iter()
        .enumerate()
        .map(|(i, x)| ChatMessage {
            role: form.roles[i].clone(),
            content: x.clone(),
        })
        .collect();

    chat_messages.push(ChatMessage {
        role: "User".to_string(),
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // build our application with a single route
    let app = Router::new()
        .route("/", get(index))
        .route("/send_message", post(send_message))
        .route("/clear_messages", post(clear_messages))
        .with_state(AppState {
            url: format!("{}/v1/chat/completions", args.llamma_cpp_server).to_string(),
        });

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("Listening on {}", listener.local_addr()?);
    Ok(axum::serve(listener, app).await?)
}
