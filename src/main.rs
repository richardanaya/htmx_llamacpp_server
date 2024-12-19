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
use axum_extra::response::{Css, JavaScript};
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use serde::Deserialize;
use serde::Serialize;
use std::net::SocketAddr;
use std::path::PathBuf;

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
    #[arg(long = "host", default_value = "127.0.0.1")]
    host: String,

    // HTTPS key file
    #[arg(
        long,
        value_name = "HTTPS_KEY_FILE",
        help = "HTTPS key file (optional)"
    )]
    https_key_file: Option<PathBuf>,

    // HTTPS cert file
    #[arg(
        long,
        value_name = "HTTPS_CERT_FILE",
        help = "HTTPS cert file (optional)"
    )]
    https_cert_file: Option<PathBuf>,
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

#[derive(Template)]
#[template(path = "chat/message.jinja")]
#[derive(Deserialize, Serialize, Clone)]
struct ModifyChatMessage {
    id: String,
    role: String,
    content: String,
}

#[derive(Template)]
#[template(path = "chat/message_edit.jinja")]
#[derive(Deserialize, Serialize, Clone)]
struct EditChatMessage {
    id: String,
    role: String,
    content: String,
}

#[derive(Deserialize, Serialize)]
struct ClearMessagesPayload {
    context: String,
}

#[derive(Deserialize, Serialize)]
struct SendMessageRequest {
    content: Option<Vec<String>>,
    role: Option<Vec<String>>,
    context: String,
    user_message: String,
    regenerate_index: Option<String>,
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

#[derive(Deserialize)]
struct ExpandPromptRequest {
    context: String,
}

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

    if name.is_some() {
        render_template(ChatTemplate {
            messages: vec![],
            context: "".to_string(),
            user_message: "".to_string(),
        })
    } else {
        render_template(LoginTemplate {})
    }
}

async fn send_ai_message(url: &str, messages: Vec<ChatMessage>) -> Result<String, StatusCode> {
    let client = reqwest::Client::new();

    let data = LlamaRequest {
        model: "gpt-3.5-turbo".to_string(),
        messages,
    };

    let response = match client.post(url).json(&data).send().await {
        Ok(x) => x,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let body = match response.json::<LlamaResponse>().await {
        Ok(x) => x,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    Ok(body.choices[0].message.content.clone())
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

    let all_roles = form.role.clone().unwrap_or(vec![]);
    let all_messages = form.content.clone().unwrap_or(vec![]);

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

    let mut ai_messages = vec![ChatMessage {
        role: "system".to_string(),
        content: form.context.clone(),
    }];
    ai_messages.extend(chat_messages.iter().cloned());

    let response = send_ai_message(&state.url, ai_messages).await?;

    chat_messages.push(ChatMessage {
        role: "AI".to_string(),
        content: response,
    });

    let context_input = format!(
        "<input type='hidden' name='context' value='{}' />",
        form.context.replace("'", "&apos;")
    );

    let chat_fragment = ChatFragmentTemplate {
        messages: chat_messages,
        context: context_input,
        user_message: "".to_string(),
    };

    render_template(chat_fragment)
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

async fn get_htmx() -> Result<JavaScript<String>> {
    Ok(JavaScript(include_str!("../static/htmx.js").to_string()))
}

async fn delete_chat_message() -> Result<Html<String>, StatusCode> {
    Ok(Html("".to_string()))
}

async fn edit_chat_message(
    Form(edit_msg): Form<EditChatMessage>,
) -> Result<Html<String>, StatusCode> {
    render_template(edit_msg)
}

async fn change_chat_message(
    Form(modify_msg): Form<ModifyChatMessage>,
) -> Result<Html<String>, StatusCode> {
    render_template(modify_msg)
}

async fn expand_prompt(
    State(state): State<AppState>,
    Form(form): Form<ExpandPromptRequest>,
) -> Result<Html<String>, StatusCode> {
    let structured_prompt = format!(
        r#"
<system_format>
You are an AI assistant specializing in expanding prompts into detailed, well-structured content.
Every response must follow this XML template format to ensure consistency and clarity.
</system_format>

<response_template>
<context>
- Original prompt intent
- Target audience
- Required depth/detail level
</context>

<main_content>
- Expanded content
- Key details and descriptions
- Logical flow of ideas
<examples>
Specific examples that illustrate the concepts
</examples>
</main_content>

<attributes>
- Tone requirements
- Style guidelines
- Consistency rules
</attributes>

<output_rules>
- Must maintain original prompt intent
- No extraneous content
- Clear section separation
- Consistent formatting
</output_rules>
</response_template>

Original prompt to expand: {}

Please provide your response using the above structure."#,
        form.context
    );

    let messages = vec![ChatMessage {
        role: "system".to_string(),
        content: structured_prompt,
    }];

    let response = send_ai_message(&state.url, messages).await?;

    Ok(Html(format!(
        "<textarea id='context' class='full' autocomplete='off' spellcheck='false' autocapitalize='off' autocorrect='off' \
         placeholder='Set AI behavior and constraints...' name='context'>{}</textarea>",
        response.replace("'", "&apos;")
    )))
}

async fn regenerate_message(
    jar: PrivateCookieJar,
    State(state): State<AppState>,
    Form(form): Form<SendMessageRequest>,
) -> Result<Html<String>, StatusCode> {
    if jar.get("name").is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let all_roles = form.role.clone().unwrap_or(vec![]);
    let all_messages = form.content.clone().unwrap_or(vec![]);

    let mut chat_messages: Vec<ChatMessage> = all_messages
        .iter()
        .enumerate()
        .map(|(i, x)| ChatMessage {
            role: all_roles[i].clone(),
            content: x.clone(),
        })
        .collect();

    if let Some(index) = form.regenerate_index {
        if let Ok(idx) = index.parse::<usize>() {
            chat_messages.truncate(idx);
        }
    }

    let mut ai_messages = vec![ChatMessage {
        role: "system".to_string(),
        content: form.context.clone(),
    }];
    ai_messages.extend(chat_messages.iter().cloned());

    let response = send_ai_message(&state.url, ai_messages).await?;

    chat_messages.push(ChatMessage {
        role: "AI".to_string(),
        content: response,
    });

    render_template(ChatFragmentTemplate {
        messages: chat_messages,
        context: form.context,
        user_message: "".to_string(),
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.https_key_file.is_some() != args.https_cert_file.is_some() {
        panic!("Must provide both HTTPS key and cert files");
    }

    // build our application with a single route
    let app = Router::new()
        .route("/", get(index).merge(post(login)).merge(delete(logout)))
        .route("/chat", post(send_message))
        .route("/chat/clear", post(clear_messages))
        .route("/chat/message", delete(delete_chat_message))
        .route("/chat/message/edit", post(edit_chat_message))
        .route("/chat/message", post(change_chat_message))
        .route("/chat/regenerate", post(regenerate_message))
        .route("/style.css", get(get_style))
        .route("/htmx.js", get(get_htmx))
        .route("/chat/expand-prompt", post(expand_prompt))
        .with_state(AppState {
            key: Key::generate(),
            url: format!("{}/v1/chat/completions", args.llamma_cpp_server).to_string(),
        });

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;

    println!("Running on https://{}", addr);

    if let (Some(key_file), Some(cert_file)) = (&args.https_key_file, &args.https_cert_file) {
        let sc = RustlsConfig::from_pem_file(cert_file, key_file).await?;
        axum_server::bind_rustls(addr, sc)
            .serve(app.into_make_service())
            .await?;
    } else {
        let cert = rcgen::generate_simple_self_signed(vec![args.host.to_owned()]).unwrap();
        let cert_file = cert.serialize_der().unwrap();
        let key_file = cert.serialize_private_key_der();
        let sc = RustlsConfig::from_der(vec![cert_file], key_file).await?;
        axum_server::bind_rustls(addr, sc)
            .serve(app.into_make_service())
            .await?;
    }

    Ok(())
}
