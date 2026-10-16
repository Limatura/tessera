//! What a preview says, in each language the server supports.
//!
//! Limited to the phrases previews and cards use, one table per language, so
//! a missing translation is a compile error instead of an English phrase on a
//! Chinese card. The wording follows the web app's messages where they say the
//! same thing.

use crate::config::Language;

/// Every phrase a preview is made of, in one language.
#[derive(Debug)]
pub struct Words {
    /// The headline of a share that has ended.
    pub gone_title: &'static str,
    /// The sentence under it.
    pub gone_description: &'static str,
    /// The headline of a locked share with no title of its own.
    pub locked_title: &'static str,
    /// The sentence under a locked share.
    pub locked_description: &'static str,
    /// The headline of an end-to-end encrypted share.
    pub sealed_title: &'static str,
    /// The headline of one file with no name.
    pub shared_file: &'static str,
    /// The headline of several files.
    pub shared_files: fn(usize) -> String,
    /// The headline of a note with no title.
    pub shared_note: &'static str,
    /// The headline of a link with no title.
    pub shared_link: &'static str,
    /// The eyebrow above several files.
    pub label_folder: &'static str,
    /// The eyebrow above one file.
    pub label_file: &'static str,
    /// The eyebrow above a note.
    pub label_note: &'static str,
    /// The eyebrow above a link.
    pub label_link: &'static str,
    /// The eyebrow on an encrypted share's card.
    pub label_sealed: &'static str,
    /// The eyebrow on a locked share's card.
    pub label_locked: &'static str,
    /// The eyebrow on an ended share's card.
    pub label_gone: &'static str,
    /// The line along the bottom of a locked share's card.
    pub locked_footer: &'static str,
    /// The line along the bottom of an ended share's card.
    pub gone_footer: &'static str,
    /// The chip counting several files.
    pub files: fn(usize) -> String,
    /// How much text a note holds, given its size.
    pub text_of: fn(&str) -> String,
    /// Expiry within the next hour.
    pub expires_within_hour: &'static str,
    /// Expiry in two hours or more.
    pub expires_in_hours: fn(i64) -> String,
    /// Expiry in one day.
    pub expires_tomorrow: &'static str,
    /// Expiry in two days or more.
    pub expires_in_days: fn(i64) -> String,
    /// How many downloads are left.
    pub left: fn(u32) -> String,
    /// Under a note's first lines, when there are more.
    pub and_more: &'static str,
}

/// The phrases for `language`.
#[must_use]
pub const fn words(language: Language) -> &'static Words {
    match language {
        Language::En => &EN,
        Language::ZhTw => &ZH_TW,
        Language::Ja => &JA,
    }
}

static EN: Words = Words {
    gone_title: "Share unavailable",
    gone_description: "This share has expired or has already been collected.",
    locked_title: "Password-protected share",
    locked_description: "Open the link and enter the password to see what this is.",
    sealed_title: "End-to-end encrypted share",
    shared_file: "Shared file",
    shared_files: |count| format!("{count} shared files"),
    shared_note: "Shared note",
    shared_link: "Shared link",
    label_folder: "SHARED FOLDER",
    label_file: "SHARED FILE",
    label_note: "SHARED NOTE",
    label_link: "SHARED LINK",
    label_sealed: "END-TO-END ENCRYPTED",
    label_locked: "PASSWORD PROTECTED",
    label_gone: "NO LONGER AVAILABLE",
    locked_footer: "Enter the password to see what this is",
    gone_footer: "It expired, or it has already been collected",
    files: |count| format!("{count} files"),
    text_of: |size| format!("{size} of text"),
    expires_within_hour: "expires within the hour",
    expires_in_hours: |hours| format!("expires in {hours} hours"),
    expires_tomorrow: "expires tomorrow",
    expires_in_days: |days| format!("expires in {days} days"),
    left: |left| {
        if left == 1 {
            "one retrieval left".to_owned()
        } else {
            format!("{left} retrievals left")
        }
    },
    and_more: "…and more",
};

static ZH_TW: Words = Words {
    gone_title: "此分享已失效",
    gone_description: "它已過期，或已達到可提取的次數上限。",
    locked_title: "受密碼保護的分享",
    locked_description: "打開連結並輸入密碼，即可查看內容。",
    sealed_title: "端對端加密的分享",
    shared_file: "分享的檔案",
    shared_files: |count| format!("{count} 個分享的檔案"),
    shared_note: "分享的文字",
    shared_link: "分享的連結",
    label_folder: "分享的資料夾",
    label_file: "分享的檔案",
    label_note: "分享的文字",
    label_link: "分享的連結",
    label_sealed: "端對端加密",
    label_locked: "受密碼保護",
    label_gone: "已失效",
    locked_footer: "輸入密碼即可查看內容",
    gone_footer: "已過期，或已達到可提取的次數上限",
    files: |count| format!("{count} 個檔案"),
    text_of: |size| format!("{size} 文字"),
    expires_within_hour: "一小時內到期",
    expires_in_hours: |hours| format!("{hours} 小時後到期"),
    expires_tomorrow: "明天到期",
    expires_in_days: |days| format!("{days} 天後到期"),
    left: |left| format!("剩餘 {left} 次提取"),
    and_more: "…還有更多",
};

static JA: Words = Words {
    gone_title: "この共有は失効しました",
    gone_description: "有効期限が切れたか、取得可能な回数に達しました。",
    locked_title: "パスワードで保護された共有",
    locked_description: "リンクを開いてパスワードを入力すると、内容を確認できます。",
    sealed_title: "エンドツーエンド暗号化された共有",
    shared_file: "共有ファイル",
    shared_files: |count| format!("{count} 件の共有ファイル"),
    shared_note: "共有テキスト",
    shared_link: "共有リンク",
    label_folder: "共有フォルダー",
    label_file: "共有ファイル",
    label_note: "共有テキスト",
    label_link: "共有リンク",
    label_sealed: "エンドツーエンド暗号化",
    label_locked: "パスワード保護",
    label_gone: "失効済み",
    locked_footer: "パスワードを入力すると内容を確認できます",
    gone_footer: "有効期限が切れたか、取得可能な回数に達しました",
    files: |count| format!("{count} ファイル"),
    text_of: |size| format!("テキスト {size}"),
    expires_within_hour: "1 時間以内に期限切れ",
    expires_in_hours: |hours| format!("{hours} 時間後に期限切れ"),
    expires_tomorrow: "明日期限切れ",
    expires_in_days: |days| format!("{days} 日後に期限切れ"),
    left: |left| format!("残り {left} 回取得可能"),
    and_more: "…続きあり",
};
