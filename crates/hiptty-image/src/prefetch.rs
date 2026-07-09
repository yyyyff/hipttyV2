use hiptty_core::{ContentNode, Post, ThreadSummary};

use crate::cache::{ImageCache, ImageKind};
use crate::smiley::prefetch_post_smileys;

#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    pub kind: ImageKind,
}

pub fn post_image_jobs(post: &Post, content_width: u16) -> Vec<FetchRequest> {
    let mut out = Vec::new();
    let max_cols = content_width.saturating_sub(2);
    if let Some(url) = &post.avatar_url {
        out.push(FetchRequest {
            url: url.clone(),
            kind: ImageKind::Avatar,
        });
    }
    for node in &post.content {
        collect_node_jobs(node, max_cols, &mut out);
    }
    out
}

pub fn prefetch_post(cache: &mut ImageCache, post: &Post, content_width: u16) -> Vec<FetchRequest> {
    prefetch_post_smileys(cache, post);
    post_image_jobs(post, content_width)
        .into_iter()
        .filter(|job| cache.request(job.url.clone(), job.kind))
        .collect()
}

/// Prefetch images for a contiguous floor range (inclusive `start`..=`end`).
pub fn prefetch_posts_range(
    cache: &mut ImageCache,
    posts: &[Post],
    start: usize,
    end: usize,
    content_width: u16,
) -> Vec<FetchRequest> {
    if posts.is_empty() {
        return Vec::new();
    }
    let end = end.min(posts.len().saturating_sub(1));
    let start = start.min(end);
    let mut jobs = Vec::new();
    for post in &posts[start..=end] {
        jobs.extend(prefetch_post(cache, post, content_width));
    }
    jobs
}

pub fn thread_avatar_job(thread: &ThreadSummary) -> Option<FetchRequest> {
    let url = thread.avatar_url.as_ref()?;
    Some(FetchRequest {
        url: url.clone(),
        kind: ImageKind::Avatar,
    })
}

pub fn prefetch_thread_avatar(
    cache: &mut ImageCache,
    thread: &ThreadSummary,
) -> Option<FetchRequest> {
    let job = thread_avatar_job(thread)?;
    if cache.request(job.url.clone(), job.kind) {
        Some(job)
    } else {
        None
    }
}

fn collect_node_jobs(node: &ContentNode, max_cols: u16, out: &mut Vec<FetchRequest>) {
    match node {
        ContentNode::Text { .. } => {}
        ContentNode::Image { url, thumb_url, .. } => {
            let image_url = thumb_url.as_deref().unwrap_or(url.as_str());
            out.push(FetchRequest {
                url: image_url.to_string(),
                kind: ImageKind::Content { max_cols },
            });
        }
        _ => {}
    }
}
