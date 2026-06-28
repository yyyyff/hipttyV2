use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use hiptty_core::PostAction;

#[derive(Debug, Parser)]
#[command(
    name = "hiptty",
    version,
    about = "Headless 4d4y forum client (agent-ready)",
    long_about = None
)]
pub struct Cli {
    /// Config directory (default: ~/.config/hiptty)
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Profile name within config directory
    #[arg(long, global = true, default_value = "default")]
    pub profile: String,

    /// Human-readable output instead of JSON
    #[arg(long, global = true)]
    pub human: bool,

    /// Verbose logging to stderr
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(subcommand)]
    Auth(AuthCmd),
    #[command(subcommand)]
    Forums(ForumsCmd),
    #[command(subcommand)]
    Threads(ThreadsCmd),
    #[command(subcommand)]
    Thread(ThreadCmd),
    Search(SearchArgs),
    #[command(subcommand)]
    Me(MeCmd),
    #[command(subcommand)]
    Favorites(FavoritesCmd),
    #[command(subcommand)]
    Attention(AttentionCmd),
    #[command(subcommand)]
    Pm(PmCmd),
    #[command(subcommand)]
    Notify(NotifyCmd),
    #[command(subcommand)]
    User(UserCmd),
    #[command(subcommand)]
    Blacklist(BlacklistCmd),
    NewPosts(NewPostsArgs),
    #[command(subcommand)]
    Post(PostCmd),
    #[command(subcommand)]
    Image(ImageCmd),
    #[command(subcommand)]
    Admin(AdminCmd),
}

#[derive(Debug, Subcommand)]
pub enum AuthCmd {
    /// Login (interactive prompts when args omitted; password hidden from shell history)
    Login {
        /// Username (prompted if omitted and stdin is a TTY)
        username: Option<String>,
        /// Password (prompted with hidden input if omitted; or HIPTTY_PASSWORD env)
        #[arg(long, env = "HIPTTY_PASSWORD")]
        password: Option<String>,
        /// Security question id 0–7 (prompted if omitted and stdin is a TTY)
        #[arg(long = "question-id")]
        question_id: Option<String>,
        /// Security answer (prompted when question-id > 0)
        #[arg(long)]
        answer: Option<String>,
    },
    /// Clear session
    Logout,
    /// Check login status
    Status,
}

#[derive(Debug, Subcommand)]
pub enum ForumsCmd {
    /// List all forums
    List,
}

#[derive(Debug, Subcommand)]
pub enum ThreadsCmd {
    /// List threads in a forum
    List {
        #[arg(long)]
        fid: u32,
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
}

#[derive(Debug, Subcommand)]
pub enum ThreadCmd {
    /// Show thread detail
    Show {
        tid: String,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, conflicts_with = "last")]
        at_pid: Option<String>,
        #[arg(long, conflicts_with = "at_pid")]
        last: bool,
    },
}

#[derive(Debug, Parser)]
pub struct SearchArgs {
    pub query: String,
    #[arg(long)]
    pub author: Option<String>,
    #[arg(long)]
    pub fid: Option<String>,
    #[arg(long)]
    pub fulltext: bool,
    #[arg(long, default_value_t = 1)]
    pub page: u32,
}

#[derive(Debug, Subcommand)]
pub enum MeCmd {
    /// My threads
    Threads {
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
    /// My replies
    Replies {
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
}

#[derive(Debug, Subcommand)]
pub enum FavoritesCmd {
    List {
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
    Add {
        tid: String,
    },
    Remove {
        tid: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum AttentionCmd {
    List {
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
}

#[derive(Debug, Subcommand)]
pub enum PmCmd {
    /// All private-message conversations
    List,
    /// Unread conversations only
    New,
    /// Lightweight check for new private messages
    Check,
    Show {
        uid: String,
    },
    Send {
        uid: String,
        content: String,
    },
    /// Delete entire conversation with a user (irreversible)
    Delete {
        uid: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum NotifyCmd {
    List,
}

#[derive(Debug, Subcommand)]
pub enum UserCmd {
    Show { uid: String },
}

#[derive(Debug, Subcommand)]
pub enum BlacklistCmd {
    List,
    Add { username: String },
    Remove { username: String },
}

#[derive(Debug, Parser)]
pub struct NewPostsArgs {
    #[arg(long)]
    pub search_id: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub page: u32,
}

#[derive(Debug, Subcommand)]
pub enum PostCmd {
    /// Fetch formhash and quote text before posting
    Prepare {
        #[arg(value_enum)]
        action: PostActionArg,
    },
    Reply {
        tid: String,
        content: String,
    },
    /// Reply to a specific post (floor); uses `reppost=pid` (not a full quote block)
    #[command(name = "reply-to")]
    ReplyTo {
        tid: String,
        pid: String,
        content: String,
    },
    Quote {
        tid: String,
        pid: String,
        content: String,
    },
    New {
        fid: u32,
        subject: String,
        content: String,
        #[arg(long)]
        type_id: Option<String>,
    },
    Edit {
        tid: String,
        pid: String,
        fid: u32,
        content: String,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long)]
        subject: Option<String>,
        #[arg(long)]
        delete: bool,
    },
    Delete {
        tid: String,
        pid: String,
        fid: u32,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PostActionArg {
    ReplyThread,
    ReplyPost,
    QuotePost,
    NewThread,
    EditPost,
}

impl From<PostActionArg> for PostAction {
    fn from(arg: PostActionArg) -> Self {
        match arg {
            PostActionArg::ReplyThread => PostAction::ReplyThread { tid: String::new() },
            PostActionArg::ReplyPost => PostAction::ReplyPost {
                tid: String::new(),
                pid: String::new(),
            },
            PostActionArg::QuotePost => PostAction::QuotePost {
                tid: String::new(),
                pid: String::new(),
            },
            PostActionArg::NewThread => PostAction::NewThread {
                fid: 0,
                type_id: None,
            },
            PostActionArg::EditPost => PostAction::EditPost {
                tid: String::new(),
                pid: String::new(),
                fid: 0,
                page: 1,
            },
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum ImageCmd {
    Upload {
        path: PathBuf,
        /// Thread ID used to open the reply form and obtain upload credentials
        #[arg(long)]
        tid: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum AdminCmd {
    /// Download HTML fixture for adapter development
    DumpFixture {
        url: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}
