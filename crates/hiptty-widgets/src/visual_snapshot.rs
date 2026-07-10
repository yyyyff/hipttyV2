//! Ratatui buffer visual snapshots for UI density / content-state regressions.
//!
//! These tests pin key glyphs and layout at 40×12, 80×24, and 120×40 so selection
//! bars, placeholders, and progressive density do not silently regress.

#[cfg(test)]
mod tests {
    use crate::content_state::{draw_content_placeholder, ContentPlaceholderKind};
    use crate::forum_tabs::{draw_forum_tabs, ForumTabsProps};
    use crate::thread_list::{draw_thread_list, ThreadListProps, ITEM_HEIGHT};
    use crate::toast::{draw_toast, ToastProps};
    use hiptty_core::ThreadSummary;
    use hiptty_render::Palette;
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let area = buf.area();
        let mut out = String::new();
        for y in 0..area.height {
            let mut x = 0u16;
            while x < area.width {
                let cell = &buf[(x, y)];
                let sym = cell.symbol();
                out.push_str(sym);
                // Skip placeholder cells under multi-width glyphs (CJK, box chars, etc.).
                let w = unicode_width::UnicodeWidthStr::width(sym).max(1) as u16;
                x = x.saturating_add(w);
            }
            out.push('\n');
        }
        out
    }

    fn sample_thread(title: &str) -> ThreadSummary {
        ThreadSummary {
            tid: "1".into(),
            title: title.into(),
            title_color: None,
            author: Some("alice".into()),
            author_id: None,
            avatar_url: None,
            last_post: Some("bob".into()),
            reply_count: Some("12".into()),
            view_count: Some("340".into()),
            time_create: Some("2026-7-1 12:00".into()),
            time_update: Some("2026-7-1 12:00".into()),
            thread_type: None,
            sticky: false,
            with_pic: false,
            is_new: false,
            is_poll: false,
            max_page: 1,
        }
    }

    fn draw_size(w: u16, h: u16, f: impl FnOnce(&mut ratatui::Frame<'_>, Rect)) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                f(frame, area);
            })
            .unwrap();
        buffer_text(&terminal)
    }

    #[test]
    fn placeholder_loading_centered_at_sizes() {
        for (w, h) in [(40u16, 12u16), (80, 24), (120, 40)] {
            let text = draw_size(w, h, |frame, area| {
                draw_content_placeholder(
                    frame,
                    area,
                    Palette::default(),
                    ContentPlaceholderKind::Loading,
                    0,
                );
            });
            assert!(
                text.contains("正在加载"),
                "{w}x{h} missing loading label:\n{text}"
            );
        }
    }

    #[test]
    fn placeholder_empty_and_error() {
        let empty = draw_size(80, 24, |frame, area| {
            draw_content_placeholder(
                frame,
                area,
                Palette::default(),
                ContentPlaceholderKind::Empty {
                    title: "暂无内容",
                    hints: "r 刷新 · n 新帖",
                },
                0,
            );
        });
        assert!(empty.contains("暂无内容"));
        assert!(empty.contains("r 刷新"));

        let err = draw_size(40, 12, |frame, area| {
            draw_content_placeholder(
                frame,
                area,
                Palette::default(),
                ContentPlaceholderKind::Error {
                    message: "网络错误",
                    retry_hint: "r 重试",
                },
                0,
            );
        });
        assert!(err.contains("网络错误"));
        assert!(err.contains("r 重试"));
    }

    #[test]
    fn thread_list_selection_bar_no_bg_wash_glyph() {
        let threads = vec![sample_thread("Hello World"), sample_thread("Second")];
        for (w, h) in [(40u16, 12u16), (80, 24), (120, 40)] {
            let text = draw_size(w, h, |frame, area| {
                // Leave a content band similar to main layout content height.
                let content = Rect {
                    x: 0,
                    y: 0,
                    width: area.width,
                    height: area.height.saturating_sub(2).max(ITEM_HEIGHT * 2),
                };
                draw_thread_list(
                    frame,
                    content,
                    ThreadListProps {
                        palette: Palette::default(),
                        threads: &threads,
                        selected: 0,
                        scroll_lines: 0,
                        show_avatar: w >= 55,
                        loading: false,
                        images: None,
                        mask_cjk: false,
                    },
                );
            });
            assert!(text.contains('│'), "{w}x{h} missing focus bar:\n{text}");
            assert!(
                text.contains("Hello World"),
                "{w}x{h} missing title:\n{text}"
            );
            // Narrow: counts hidden when text column < MIN_COUNTS_COLS (~28 after bar).
            if w <= 40 {
                assert!(
                    !text.contains('\u{f27a}'),
                    "{w}x{h} should hide reply icon on narrow:\n{text}"
                );
            }
        }
    }

    #[test]
    fn forum_tabs_keep_active_on_narrow() {
        let text = draw_size(14, 1, |frame, area| {
            draw_forum_tabs(
                frame,
                area,
                ForumTabsProps {
                    palette: Palette::default(),
                    default_forums: &[2, 6, 7],
                    active_fid: 7,
                    hover_tab: None,
                    mask_cjk: false,
                },
            );
        });
        // Active forum 7 starts with "▸" and name containing Geek or 奇客.
        assert!(text.contains('▸'), "active marker missing:\n{text}");
    }

    #[test]
    fn toast_keeps_full_border_at_half_life() {
        let text = draw_size(80, 24, |frame, area| {
            draw_toast(
                frame,
                area,
                ToastProps {
                    palette: Palette::default(),
                    message: "发送成功",
                    is_error: false,
                    tick: 20,
                    started_at: 0,
                    duration_ticks: 40,
                    bottom_inset: 0,
                },
            );
        });
        // Corners from the stable perimeter should still be present at half remaining.
        assert!(
            text.contains('┌') || text.contains('┐'),
            "missing top corner:\n{text}"
        );
        assert!(
            text.contains('└') || text.contains('┘'),
            "missing bottom corner:\n{text}"
        );
        assert!(
            text.contains("发送成功") || text.contains('✓'),
            "missing message:\n{text}"
        );
    }
}
