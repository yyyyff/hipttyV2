use hiptty_adapter::ForumClient;
use hiptty_core::{AdapterError, AdapterResult, PostAction, SearchQuery};
use serde::Serialize;

use crate::cli::Cli;
use crate::output::{self, Response};

fn to_json<T: Serialize>(value: T) -> AdapterResult<serde_json::Value> {
    serde_json::to_value(value).map_err(|e| AdapterError::Parse(e.to_string()))
}

pub async fn execute(cli: Cli, client: &impl ForumClient) -> i32 {
    let human = cli.human;
    let result = dispatch(cli, client).await;
    match result {
        Ok(response) => {
            if human {
                if let Some(data) = &response.data {
                    print_human_data(data);
                } else {
                    output::print_human_ok("ok");
                }
            } else {
                output::print_json(&response);
            }
            output::exit::SUCCESS
        }
        Err(err) => {
            if human {
                output::print_human_error(&err);
            } else {
                output::print_json(&Response::<()>::failure(err.clone()));
            }
            output::exit_code_for_error(&err)
        }
    }
}

async fn dispatch(
    cli: Cli,
    client: &impl ForumClient,
) -> AdapterResult<Response<serde_json::Value>> {
    use crate::cli::{
        AdminCmd, AttentionCmd, AuthCmd, BlacklistCmd, FavoritesCmd, ForumsCmd, ImageCmd, MeCmd,
        NotifyCmd, PmCmd, PostCmd, ThreadCmd, ThreadsCmd, UserCmd,
    };

    let data = match cli.command {
        crate::cli::Commands::Auth(AuthCmd::Login {
            username,
            password,
            question_id,
            answer,
        }) => {
            let creds =
                crate::auth_prompt::gather_credentials(username, password, question_id, answer)?;
            to_json(client.login(creds).await?)?
        }
        crate::cli::Commands::Auth(AuthCmd::Logout) => {
            client.logout().await?;
            to_json(serde_json::json!({ "logged_out": true }))?
        }
        crate::cli::Commands::Auth(AuthCmd::Status) => to_json(client.session_status().await?)?,
        crate::cli::Commands::Forums(ForumsCmd::List) => to_json(hiptty_core::FORUMS)?,
        crate::cli::Commands::Threads(ThreadsCmd::List { fid, page }) => {
            to_json(client.forum_threads(fid, page).await?)?
        }
        crate::cli::Commands::Thread(ThreadCmd::Show {
            tid,
            page,
            at_pid,
            last,
        }) => {
            let detail = if last {
                client.thread_last_page(&tid).await?
            } else if let Some(pid) = at_pid {
                client.thread_at_post(&tid, &pid).await?
            } else {
                client.thread_detail(&tid, page).await?
            };
            to_json(detail)?
        }
        crate::cli::Commands::Search(args) => {
            let mut q = SearchQuery::new(args.query);
            q.author = args.author;
            q.fid = args.fid;
            q.fulltext = args.fulltext;
            q.page = args.page;
            to_json(client.search(q).await?)?
        }
        crate::cli::Commands::Me(MeCmd::Threads { page }) => {
            to_json(client.my_threads(page).await?)?
        }
        crate::cli::Commands::Me(MeCmd::Replies { page }) => {
            to_json(client.my_replies(page).await?)?
        }
        crate::cli::Commands::Favorites(FavoritesCmd::List { page }) => {
            to_json(client.favorites(page).await?)?
        }
        crate::cli::Commands::Favorites(FavoritesCmd::Add { tid }) => {
            client.favorite_add(&tid).await?;
            to_json(serde_json::json!({ "tid": tid, "added": true }))?
        }
        crate::cli::Commands::Favorites(FavoritesCmd::Remove { tid }) => {
            client.favorite_remove(&tid).await?;
            to_json(serde_json::json!({ "tid": tid, "removed": true }))?
        }
        crate::cli::Commands::Attention(AttentionCmd::List { page }) => {
            to_json(client.attention(page).await?)?
        }
        crate::cli::Commands::Pm(PmCmd::List) => to_json(client.pm_list().await?)?,
        crate::cli::Commands::Pm(PmCmd::New) => to_json(client.pm_new_list().await?)?,
        crate::cli::Commands::Pm(PmCmd::Check) => {
            let has_new = client.check_new_pm().await?;
            to_json(serde_json::json!({ "has_new": has_new }))?
        }
        crate::cli::Commands::Pm(PmCmd::Show { uid }) => to_json(client.pm_thread(&uid).await?)?,
        crate::cli::Commands::Pm(PmCmd::Send { uid, content }) => {
            client.send_pm(&uid, &content).await?;
            to_json(serde_json::json!({ "uid": uid, "sent": true }))?
        }
        crate::cli::Commands::Pm(PmCmd::Delete { uid }) => {
            client.pm_delete(&uid).await?;
            to_json(serde_json::json!({ "uid": uid, "deleted": true }))?
        }
        crate::cli::Commands::Notify(NotifyCmd::List) => to_json(client.notifications().await?)?,
        crate::cli::Commands::User(UserCmd::Show { uid }) => {
            to_json(client.user_info(&uid).await?)?
        }
        crate::cli::Commands::Blacklist(BlacklistCmd::List) => to_json(client.blacklist().await?)?,
        crate::cli::Commands::Blacklist(BlacklistCmd::Add { username }) => {
            client.blacklist_add(&username).await?;
            to_json(serde_json::json!({ "username": username, "added": true }))?
        }
        crate::cli::Commands::Blacklist(BlacklistCmd::Remove { username }) => {
            client.blacklist_remove(&username).await?;
            to_json(serde_json::json!({ "username": username, "removed": true }))?
        }
        crate::cli::Commands::NewPosts(args) => to_json(
            client
                .new_posts(args.search_id.as_deref(), args.page)
                .await?,
        )?,
        crate::cli::Commands::Post(PostCmd::Prepare { action }) => {
            to_json(client.prepare_post(action.into()).await?)?
        }
        crate::cli::Commands::Post(PostCmd::Reply { tid, content }) => to_json(
            client
                .post(PostAction::ReplyThread { tid }, &content, None, false)
                .await?,
        )?,
        crate::cli::Commands::Post(PostCmd::ReplyTo { tid, pid, content }) => to_json(
            client
                .post(PostAction::ReplyPost { tid, pid }, &content, None, false)
                .await?,
        )?,
        crate::cli::Commands::Post(PostCmd::Quote { tid, pid, content }) => to_json(
            client
                .post(PostAction::QuotePost { tid, pid }, &content, None, false)
                .await?,
        )?,
        crate::cli::Commands::Post(PostCmd::New {
            fid,
            content,
            subject,
            type_id,
        }) => to_json(
            client
                .post(
                    PostAction::NewThread { fid, type_id },
                    &content,
                    Some(&subject),
                    false,
                )
                .await?,
        )?,
        crate::cli::Commands::Post(PostCmd::Edit {
            tid,
            pid,
            fid,
            page,
            content,
            subject,
            delete,
        }) => to_json(
            client
                .post(
                    PostAction::EditPost {
                        tid,
                        pid,
                        fid,
                        page,
                    },
                    &content,
                    subject.as_deref(),
                    delete,
                )
                .await?,
        )?,
        crate::cli::Commands::Post(PostCmd::Delete { tid, pid, fid }) => to_json(
            client
                .post(PostAction::QuickDelete { tid, pid, fid }, "", None, true)
                .await?,
        )?,
        crate::cli::Commands::Image(ImageCmd::Upload { path, tid }) => {
            let bytes = std::fs::read(&path).map_err(|e| {
                AdapterError::InvalidInput(format!("cannot read {}: {e}", path.display()))
            })?;
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("image.jpg");
            let img_id = client
                .upload_image(PostAction::ReplyThread { tid }, &bytes, filename)
                .await?;
            to_json(serde_json::json!({ "image_id": img_id }))?
        }
        crate::cli::Commands::Admin(AdminCmd::DumpFixture { url, output }) => {
            let dump = client.dump_fixture(&url, output.as_deref()).await?;
            to_json(dump)?
        }
    };

    Ok(Response::success(data))
}

fn print_human_data(data: &serde_json::Value) {
    match data {
        serde_json::Value::Array(forums) if forums.iter().all(|f| f.get("id").is_some()) => {
            for forum in forums {
                println!(
                    "{} — {}",
                    forum["id"].as_u64().unwrap_or(0),
                    forum["name"].as_str().unwrap_or("")
                );
            }
        }
        serde_json::Value::Object(obj) if obj.get("posts").and_then(|v| v.as_array()).is_some() => {
            println!(
                "{} (page {}/{})",
                obj.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                obj.get("page").and_then(|v| v.as_u64()).unwrap_or(1),
                obj.get("last_page").and_then(|v| v.as_u64()).unwrap_or(1),
            );
            if let Some(posts) = obj.get("posts").and_then(|v| v.as_array()) {
                for post in posts {
                    println!(
                        "\n#{} {} @ {} — {}",
                        post.get("floor").and_then(|v| v.as_u64()).unwrap_or(0),
                        post.get("author").and_then(|v| v.as_str()).unwrap_or("?"),
                        post.get("time").and_then(|v| v.as_str()).unwrap_or(""),
                        post.get("pid").and_then(|v| v.as_str()).unwrap_or(""),
                    );
                    if let Some(content) = post.get("content").and_then(|v| v.as_array()) {
                        for node in content.iter().take(6) {
                            print_content_node_human(node);
                        }
                        if content.len() > 6 {
                            println!("  … ({} more blocks)", content.len() - 6);
                        }
                    }
                    if post.get("poll").is_some_and(|p| !p.is_null()) {
                        println!("  [投票]");
                    }
                }
            }
        }
        serde_json::Value::Object(obj) if obj.get("items").and_then(|v| v.as_array()).is_some() => {
            let page = obj.get("page").and_then(|v| v.as_u64()).unwrap_or(1);
            let max_page = obj.get("max_page").and_then(|v| v.as_u64()).unwrap_or(1);
            println!("page {page}/{max_page}");
            if let Some(items) = obj.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    let new = if item
                        .get("is_new")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        "[新] "
                    } else {
                        ""
                    };
                    let title = item
                        .get("title")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            item.get("info")
                                .and_then(|v| v.as_str())
                                .map(pm_summary_text)
                        })
                        .unwrap_or_default();
                    let author = item.get("author").and_then(|v| v.as_str()).unwrap_or("");
                    let time = item.get("time").and_then(|v| v.as_str()).unwrap_or("");
                    let forum = item.get("forum").and_then(|v| v.as_str()).unwrap_or("");
                    let tid = item.get("tid").and_then(|v| v.as_str()).unwrap_or("");
                    let pid = item.get("pid").and_then(|v| v.as_str()).unwrap_or("");
                    let mut line = format!("{new}{title}");
                    if !author.is_empty() {
                        line.push_str(&format!(" — {author}"));
                    }
                    if !time.is_empty() {
                        line.push_str(&format!(" @ {time}"));
                    }
                    if !forum.is_empty() {
                        line.push_str(&format!(" [{forum}]"));
                    }
                    if !tid.is_empty() || !pid.is_empty() {
                        line.push_str(&format!(" (tid={tid} pid={pid})"));
                    }
                    println!("{line}");
                }
            }
        }
        serde_json::Value::Object(obj)
            if obj.get("username").is_some()
                && obj.get("uid").is_some()
                && obj.get("detail").is_some() =>
        {
            let online = if obj.get("online").and_then(|v| v.as_bool()).unwrap_or(false) {
                "在线"
            } else {
                "离线"
            };
            println!(
                "{} (UID {}) — {}",
                obj.get("username").and_then(|v| v.as_str()).unwrap_or(""),
                obj.get("uid").and_then(|v| v.as_str()).unwrap_or(""),
                online,
            );
            if let Some(detail) = obj.get("detail").and_then(|v| v.as_str()) {
                println!();
                println!("{detail}");
            }
        }
        serde_json::Value::Object(obj)
            if obj.get("url").is_some()
                && obj.get("output").is_some()
                && obj.get("bytes").is_some() =>
        {
            println!(
                "saved {} ({} bytes) from {}",
                obj.get("output").and_then(|v| v.as_str()).unwrap_or(""),
                obj.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0),
                obj.get("url").and_then(|v| v.as_str()).unwrap_or(""),
            );
        }
        serde_json::Value::Array(names) if names.iter().all(|n| n.as_str().is_some()) => {
            for name in names {
                println!("{}", name.as_str().unwrap_or(""));
            }
        }
        serde_json::Value::Object(obj)
            if obj.get("threads").and_then(|v| v.as_array()).is_some() =>
        {
            let page = obj.get("page").and_then(|v| v.as_u64()).unwrap_or(1);
            let max_page = obj.get("max_page").and_then(|v| v.as_u64()).unwrap_or(1);
            println!("page {page}/{max_page}");
            if let Some(threads) = obj.get("threads").and_then(|v| v.as_array()) {
                for thread in threads {
                    let sticky = if thread
                        .get("sticky")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        "[置顶] "
                    } else {
                        ""
                    };
                    println!(
                        "{}{} — {} (回复 {})",
                        sticky,
                        thread.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                        thread.get("author").and_then(|v| v.as_str()).unwrap_or("?"),
                        thread
                            .get("reply_count")
                            .and_then(|v| v.as_str())
                            .unwrap_or("0"),
                    );
                }
            }
        }
        _ => output::print_json(data),
    }
}

fn pm_summary_text(raw: &str) -> String {
    if let Some(start) = raw.find('>') {
        if let Some(end) = raw.rfind('<') {
            if end > start {
                return raw[start + 1..end].trim().to_string();
            }
        }
    }
    raw.trim().to_string()
}

fn print_content_node_human(node: &serde_json::Value) {
    let kind = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match kind {
        "text" => {
            let text: String = node
                .get("spans")
                .and_then(|v| v.as_array())
                .map(|spans| {
                    spans
                        .iter()
                        .map(|s| match s.get("type").and_then(|v| v.as_str()) {
                            Some("smiley") => format!(
                                "[{}]",
                                s.get("code").and_then(|v| v.as_str()).unwrap_or("表情")
                            ),
                            _ => s
                                .get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            let preview: String = text.chars().take(120).collect();
            if !preview.trim().is_empty() {
                println!("  {preview}");
            }
        }
        "quote" => {
            let text = node.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let preview: String = text.chars().take(80).collect();
            println!("  > {preview}");
        }
        "image" => {
            let url = node.get("url").and_then(|v| v.as_str()).unwrap_or("");
            println!("  [图片] {url}");
        }
        "attachment" => {
            let name = node.get("name").and_then(|v| v.as_str()).unwrap_or("");
            println!("  [附件] {name}");
        }
        "floor_ref" => {
            let floor = node.get("floor").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("  [跳转 #{floor}]");
        }
        _ => {}
    }
}
