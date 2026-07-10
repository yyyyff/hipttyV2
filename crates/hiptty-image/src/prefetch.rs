use std::collections::HashSet;

use hiptty_core::{ContentNode, ContentSpan, Post, ThreadSummary};

use crate::cache::{ImageCache, ImageKind};
use crate::smiley::{prefetch_post_smileys, smiley_cache_key};

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

/// Cache keys that must stay pinned for the given posts (avatars, content images, smileys).
pub fn pin_urls_for_posts(posts: &[Post], content_width: u16) -> HashSet<String> {
    let mut urls = HashSet::new();
    for post in posts {
        for job in post_image_jobs(post, content_width) {
            urls.insert(job.url);
        }
        collect_smiley_pin_urls(post, &mut urls);
    }
    urls
}

fn collect_smiley_pin_urls(post: &Post, out: &mut HashSet<String>) {
    for node in &post.content {
        let ContentNode::Text { spans } = node else {
            continue;
        };
        for span in spans {
            if let ContentSpan::Smiley {
                url,
                code,
                smilie_id,
            } = span
            {
                out.insert(smiley_cache_key(code.as_deref(), smilie_id.as_deref(), url));
            }
        }
    }
}

pub fn prefetch_post(cache: &mut ImageCache, post: &Post, content_width: u16) -> Vec<FetchRequest> {
    prefetch_post_smileys(cache, post);
    post_image_jobs(post, content_width)
        .into_iter()
        .filter(|job| cache.request(job.url.clone(), job.kind))
        .collect()
}

/// Prefetch images for a contiguous floor range (inclusive `start`..=`end`).
///
/// Also updates the cache pin set to those posts so soft-budget eviction cannot
/// drop viewport images (which would cause re-download thrashing on tall images).
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
    let range = &posts[start..=end];
    cache.set_pinned_urls(pin_urls_for_posts(range, content_width));
    let mut jobs = Vec::new();
    for post in range {
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
