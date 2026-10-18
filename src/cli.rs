use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Command-line tool for the Commonplace REST API. Every command prints JSON.
///
/// Exit codes: 0 success, 1 error, 2 usage mistake, 3 Browser handoff (a person must finish the act in the
/// browser at `handoffUrl`).
#[derive(Parser)]
#[command(name = "commonplace", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Sign in through Auth in the browser, or with a device code, and store the sign-in.
    Login(LoginArgs),

    /// End the stored sign-in at Auth and forget it. COMMONPLACE_TOKEN is not touched.
    Logout,

    /// Show the host, the credential in use, the User, and the Account commands act in.
    Status,

    /// List the User's Accounts, or move the credential to another one.
    #[command(subcommand)]
    Accounts(AccountsCommand),

    /// Talk to our Assistant.
    #[command(subcommand)]
    Chat(ChatCommand),

    /// Make one raw call to the REST API with the current credential.
    Api(ApiArgs),

    /// Print the Agent skill file: when and how to use this CLI, as Markdown.
    Skill,
}

#[derive(Args)]
pub struct LoginArgs {
    /// Sign in with a code on another device instead of a browser on this one.
    #[arg(long)]
    pub device: bool,
}

#[derive(Subcommand)]
pub enum AccountsCommand {
    /// List every Account the User belongs to, the current one marked.
    List,

    /// Move the credential into another Account, for every later command.
    Switch {
        /// The Account id, from `commonplace accounts list`.
        id: u64,
    },
}

#[derive(Subcommand)]
pub enum ChatCommand {
    /// Send one message to our Assistant and wait for its reply.
    ///
    /// Prints `{"conversationId","turnState","lastError","queued","messages"}`: every message after the sent one, as the API
    /// gives it. Answer the Assistant's questions and proposals with the next `chat send --conversation
    /// <conversationId>`.
    Send(ChatSendArgs),
}

#[derive(Args)]
pub struct ChatSendArgs {
    /// The message, in markdown.
    pub text: String,

    /// The conversation to continue. Without it, a new conversation starts.
    #[arg(long, value_name = "ID")]
    pub conversation: Option<String>,

    /// Attach this file to the message: a PDF, image, text, Markdown, CSV, XLSX, or DOCX file of at most 25 MiB.
    /// Repeat it for more files.
    #[arg(long, value_name = "PATH")]
    pub file: Vec<PathBuf>,
}

#[derive(Args)]
pub struct ApiArgs {
    /// HTTP method, such as GET, POST, PUT, PATCH, or DELETE.
    pub method: String,

    /// Path relative to the host, such as /api/v1/books. A query string rides in the path.
    pub path: String,

    /// JSON request body, given inline or read from a file with @path.
    #[arg(long, value_name = "JSON|@FILE")]
    pub body: Option<String>,

    /// Write the response body to this file and print a summary instead.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,
}
