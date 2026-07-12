pub mod composer;
pub mod content_state;
pub mod floor_list;
pub mod forum_picker;
pub mod forum_tabs;
pub mod ime;
pub mod layout;
pub mod login;
pub mod logo;
pub mod modal;
pub mod overlays;
pub mod pm_thread;
pub mod poll_block;
pub mod scroll;
pub mod simple_list;
pub mod status_bar;
pub mod thread_list;
pub mod title_bar;
pub mod toast;
#[cfg(test)]
mod visual_snapshot;

pub use composer::{
    composer_height, draw_composer, draw_confirm_dialog, ComposerFocus, ComposerProps, ConfirmProps,
};
pub use content_state::{draw_content_placeholder, list_placeholder, ContentPlaceholderKind};
pub use floor_list::{
    capture_detail_scroll_anchor, clamp_scroll_top, detail_line_scroll, detail_step_down,
    detail_step_up, draw_floor_list, ensure_scroll_top, first_visible_floor, floor_index_at_line,
    floor_list_total_height, floor_offsets, last_visible_floor, measure_floor, page_scroll_top,
    restore_detail_scroll_anchor, DetailScrollAnchor, DocY, FloorLayout, FloorListProps,
};
pub use forum_picker::{
    draw_forum_picker, forum_picker_entries, ForumPickerFrame, ForumPickerHit, ForumPickerProps,
};
pub use forum_tabs::{draw_forum_tabs, forum_tab_hits, ForumTabHits, ForumTabsProps};
pub use ime::{cursor_after_text, next_scroll_top, set_ime_cursor, textarea_cursor_position};
pub use layout::{draw_dim_rule, main_layout};
pub use login::{draw_login, draw_startup, LoginField, LoginFormProps, StartupProps};
pub use overlays::{
    draw_main_menu, draw_search_prompt, draw_settings_panel, MainMenuProps, SearchPromptProps,
    SettingsProps, MAIN_MENU_HINTS, MAIN_MENU_ITEMS,
};
pub use pm_thread::{draw_pm_thread, pm_thread_capacity, PmThreadProps, PM_ITEM_HEIGHT};
pub use scroll::{
    align_scroll_to_item_top, apply_scroll_delta, apply_scroll_delta_u32,
    clamp_thread_scroll_lines, draw_vertical_scrollbar, ensure_thread_scroll_lines,
    item_index_at_row, list_content_lines, max_scroll_lines, max_scroll_lines_u32,
    snap_scroll_to_item, split_content_scrollbar, ScrollBar, ScrollBarArrows, ScrollBarInteraction,
    ScrollChrome, ScrollCommand, SCROLLBAR_COLS, WHEEL_LINES,
};
pub use simple_list::{
    draw_simple_list, simple_list_capacity, SimpleListProps, SIMPLE_ITEM_HEIGHT,
};
pub use status_bar::{
    draw_status_bar, fit_hints, loading_status_label, page_status_label, CommandLineProps, KeyHint,
    StatusBarProps,
};
pub use thread_list::{
    draw_loading_indicator, draw_thread_list, ensure_thread_scroll,
    ensure_thread_scroll_lines as ensure_list_scroll_lines, thread_list_capacity, ThreadListProps,
    ITEM_HEIGHT,
};
pub use title_bar::{
    draw_title_bar, title_bar_hits, TitleBarHits, TitleBarProps, TitleUnreadHover,
};
pub use toast::{draw_toast, ToastProps, TOAST_ERROR_TICKS, TOAST_SUCCESS_TICKS, TOAST_TICK_MS};
