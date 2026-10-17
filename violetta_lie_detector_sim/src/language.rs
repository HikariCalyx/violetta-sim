//! Which language the app is being run in.
//!
//! The artwork exists in a few languages, one directory per language under
//! `Resources/`, and which one to show is the platform's answer rather than the
//! player's: the browser's own language setting, or the locale the desktop build is
//! run under. A browser that asks for English — or for something nothing here is
//! translated into — gets the base artwork, which *is* the English one, so there is
//! nothing to fall back to and nothing to fail.

use crate::assets;

/// The language the app should show, if the build carries artwork for it.
pub fn detect() -> Option<&'static str> {
    let tag = tag()?;

    let language = match_tag(&tag);
    match language {
        Some(language) => log::info!("showing the `{language}` artwork, for `{tag}`"),
        None => log::info!("no artwork for `{tag}`, showing the base artwork"),
    }

    language
}

/// The language tag the platform is set to.
#[cfg(target_arch = "wasm32")]
fn tag() -> Option<String> {
    // `navigator.language` is the browser's own preference, e.g. `ko-KR`.
    web_sys::window()?.navigator().language()
}

/// The language tag the platform is set to: the locale variables a desktop run
/// inherits, which is all a native build has to go on.
#[cfg(not(target_arch = "wasm32"))]
fn tag() -> Option<String> {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
}

/// The app's own name in `language`.
///
/// The character's name is in every language's artwork — the standby dialog asks
/// the player to pick her out — so these follow it: `lieDetectorStandBy.back`
/// reads `ビオルタ` for Japanese, `비올레타` for Korean, `碧欧蕾塔` for Simplified
/// Chinese, and `菲歐娜` for Traditional Chinese, where the localisation renamed
/// her. The rest of each name is the game's own wording for the device in that
/// language, plus the word for a simulator. English is the base artwork, and the
/// name the app is built as.
pub fn title(language: Option<&'static str>) -> &'static str {
    match language {
        Some("cn") => "碧欧蕾塔测谎模拟器",
        Some("jp") => "ビオルタマクロ探知機シミュレーター",
        Some("kr") => "비올레타 거짓말 탐지 시뮬레이터",
        Some("tw") => "菲歐娜測謊模擬",
        _ => "Violetta Lie Detector Simulator",
    }
}

/// The words the app writes itself, rather than reading them out of the artwork:
/// the buttons, what the result screen reports, and the chrome the bot's own screens
/// put around a verification.
///
/// They are what the game's own wording is for, so they are short: the buttons, a
/// caption over an outcome, and the lines the bot's verification adds.
#[derive(Clone, Copy, Debug)]
pub struct Words {
    /// The button that starts a game.
    pub start: &'static str,
    /// The name of the chaos-mode button, where the language has one of its own. A
    /// language that leaves it `None` keeps the English name on the button.
    pub chaos: Option<&'static str>,
    /// The button that starts another one.
    pub try_again: &'static str,
    /// How a game the player won is reported.
    pub success: &'static str,
    /// How one the player lost is reported.
    pub failed: &'static str,
    /// What the page says while a verification's session is still being read.
    pub checking: &'static str,
    /// What it says when the session could not be read at all; the reason the Worker
    /// gave follows on the next line.
    pub unavailable: &'static str,
    /// The headline for a verification the member passed.
    pub verified: &'static str,
    /// The line under it, saying what that means for them.
    pub verified_detail: &'static str,
    /// The headline for a verification that used up its attempts.
    pub out_of_tries: &'static str,
    /// The headline for one whose clock ran out.
    pub time_up: &'static str,
    /// What both of those are followed by: the member is out of the group, and
    /// rejoining is what gets them another go.
    pub removed: &'static str,
    /// How many attempts are left, with `$count` standing in for the number.
    pub tries_left: &'static str,
    /// The same line for a single attempt, which English is the only language here
    /// to write differently — `1 try left`. `None` means `tries_left` covers every
    /// count.
    pub try_left: Option<&'static str>,
}

impl Words {
    /// The retry line: how many attempts are left, in the wording this language
    /// uses.
    ///
    /// English is the one language here that inflects for a count, which is why it
    /// carries a second wording for a single attempt; the others write the number
    /// into their one template. The verification clock and the result screen both
    /// come through here, so they say the same thing.
    pub fn tries_left(&self, count: u32) -> String {
        let template = match count {
            1 => self.try_left.unwrap_or(self.tries_left),
            _ => self.tries_left,
        };

        template.replace("$count", &count.to_string())
    }
}

/// The words the app writes in `language`.
///
/// `scripts/subset-cjk-font.py` reads the strings out of this table to know which
/// glyphs the embedded fonts have to hold, so each one has to be a literal here
/// rather than something built at run time. English is the base artwork and the
/// language the app is written in, so it is what everything else falls back to.
pub fn words(language: Option<&'static str>) -> Words {
    match language {
        Some("cn") => Words {
            start: "开始",
            chaos: Some("进阶模式"),
            try_again: "再试一次",
            success: "成功",
            failed: "失败",
            checking: "正在读取验证信息…",
            unavailable: "此验证已失效。",
            verified: "已验证",
            verified_detail: "现在你可以在群里发言了。",
            out_of_tries: "机会已用尽",
            time_up: "时间到了",
            removed: "你已被移出群聊，可以重新加入再试一次。",
            tries_left: "剩余 $count 次机会",
            try_left: None,
        },
        Some("jp") => Words {
            start: "スタート",
            chaos: Some("カオスモード"),
            try_again: "もう一度",
            success: "成功",
            failed: "失敗",
            checking: "認証情報を確認しています…",
            unavailable: "この認証は利用できません。",
            verified: "認証完了",
            verified_detail: "これでグループで発言できます。",
            out_of_tries: "挑戦回数がなくなりました",
            time_up: "時間切れ",
            removed: "グループから退出させられました。再度参加してもう一度お試しください。",
            tries_left: "残り $count 回",
            try_left: None,
        },
        Some("kr") => Words {
            start: "시작",
            chaos: Some("카오스 모드"),
            try_again: "다시하기",
            success: "성공",
            failed: "실패",
            checking: "인증 정보를 확인하는 중…",
            unavailable: "이 인증을 사용할 수 없습니다.",
            verified: "인증 완료",
            verified_detail: "이제 그룹에서 발언할 수 있습니다.",
            out_of_tries: "남은 기회가 없습니다",
            time_up: "시간 초과",
            removed: "그룹에서 내보내졌습니다. 다시 참여해 재시도할 수 있습니다.",
            tries_left: "$count회 남음",
            try_left: None,
        },
        Some("tw") => Words {
            start: "開始",
            chaos: Some("混沌模式"),
            try_again: "再試一次",
            success: "成功",
            failed: "失敗",
            checking: "正在讀取驗證資訊…",
            unavailable: "此驗證已失效。",
            verified: "已驗證",
            verified_detail: "現在你可以在群組裡發言了。",
            out_of_tries: "機會已用盡",
            time_up: "時間到了",
            removed: "你已被移出群組，可以重新加入再試一次。",
            tries_left: "剩餘 $count 次機會",
            try_left: None,
        },
        _ => Words {
            start: "Start",
            chaos: None,
            try_again: "Try Again",
            success: "Success",
            failed: "Failed",
            checking: "Checking your verification…",
            unavailable: "This verification is unavailable.",
            verified: "Verified",
            verified_detail: "You can talk in the group now.",
            out_of_tries: "Out of tries",
            time_up: "Time is up",
            removed: "You were removed from the group. You can rejoin and try again.",
            tries_left: "$count tries left",
            try_left: Some("1 try left"),
        },
    }
}

/// Which of the shipped languages `tag` asks for, if the artwork for it is there.
///
/// A tag is a primary subtag and then whatever the platform cared to add
/// (`ko-KR`, `zh-Hant-TW`, `ja_JP.UTF-8`), so only the parts that mean something
/// here are read: the language, and for Chinese the region or the script, since
/// that is what decides between the two Chinese translations.
fn match_tag(tag: &str) -> Option<&'static str> {
    let mut subtags = tag
        .split(['-', '_', '.', '@'])
        .map(|subtag| subtag.to_ascii_lowercase());
    let primary = subtags.next()?;

    let language = match primary.as_str() {
        "ko" => "kr",
        "ja" => "jp",
        "zh" => {
            let mut traditional = false;
            let mut simplified = false;
            for subtag in subtags {
                match subtag.as_str() {
                    "tw" | "hk" | "mo" | "hant" => traditional = true,
                    "cn" | "sg" | "my" | "hans" => simplified = true,
                    _ => {}
                }
            }
            // A bare `zh` takes the mainland translation, which is the one more
            // Chinese readers are given.
            if traditional && !simplified {
                "tw"
            } else {
                "cn"
            }
        }
        // English is the base artwork, and an unrecognised language is better off
        // with it than with nothing.
        _ => return None,
    };

    assets::languages()
        .any(|shipped| shipped == language)
        .then_some(language)
}

#[cfg(test)]
mod tests {
    use super::match_tag;

    #[test]
    fn maps_the_tags_a_platform_reports() {
        assert_eq!(match_tag("ko-KR"), Some("kr"));
        assert_eq!(match_tag("ko"), Some("kr"));
        assert_eq!(match_tag("ja-JP"), Some("jp"));
        assert_eq!(match_tag("ja_JP.UTF-8"), Some("jp"));
        assert_eq!(match_tag("zh-CN"), Some("cn"));
        assert_eq!(match_tag("zh-Hans"), Some("cn"));
        assert_eq!(match_tag("zh"), Some("cn"));
        assert_eq!(match_tag("zh-TW"), Some("tw"));
        assert_eq!(match_tag("zh-Hant-HK"), Some("tw"));
    }

    #[test]
    fn leaves_english_and_the_unknown_on_the_base_artwork() {
        assert_eq!(match_tag("en-GB"), None);
        assert_eq!(match_tag("de"), None);
        assert_eq!(match_tag(""), None);
    }

    /// Every word the app writes for itself in `language`, named so that a failure
    /// can say which one it is: the buttons, including the mode's, the four the
    /// screens use, and the nine the bot's verification adds.
    fn localised_words(language: Option<&'static str>) -> [(&'static str, &'static str); 14] {
        let words = super::words(language);
        [
            ("start", words.start),
            // A language that has not named the mode draws the English name instead.
            ("chaos", words.chaos.unwrap_or("Chaos Mode")),
            ("try_again", words.try_again),
            ("success", words.success),
            ("failed", words.failed),
            ("checking", words.checking),
            ("unavailable", words.unavailable),
            ("verified", words.verified),
            ("verified_detail", words.verified_detail),
            ("out_of_tries", words.out_of_tries),
            ("time_up", words.time_up),
            ("removed", words.removed),
            // The wording a single attempt gets, which is the plural one wherever the
            // language does not inflect.
            ("try_left", words.try_left.unwrap_or(words.tries_left)),
            ("tries_left", words.tries_left),
        ]
    }

    #[test]
    fn every_shipped_language_has_strings_of_its_own() {
        let english = localised_words(None);

        for language in crate::assets::languages() {
            for ((word, localised), (_, base)) in
                localised_words(Some(language)).into_iter().zip(english)
            {
                assert_ne!(
                    localised, base,
                    "`{language}`'s `{word}` is still the English one"
                );
            }

            assert_ne!(
                super::title(Some(language)),
                super::title(None),
                "`{language}` has no localised name in `language::title`"
            );
        }
    }

    /// The two Chinese translations name the mode differently: the mainland one
    /// calls it the advanced mode, the Taiwanese one keeps the word for chaos.
    #[test]
    fn the_chinese_name_the_chaos_button() {
        assert_eq!(super::words(Some("cn")).chaos, Some("进阶模式"));
        assert_eq!(super::words(Some("tw")).chaos, Some("混沌模式"));
    }

    #[test]
    fn the_retry_line_is_filled_in() {
        let words = super::words(None);

        // English inflects, so it is the one language with a wording for one.
        assert_eq!(words.tries_left(1), "1 try left");
        assert_eq!(words.tries_left(0), "0 tries left");
        assert_eq!(words.tries_left(4), "4 tries left");

        // The others write the number into their one template.
        let korean = super::words(Some("kr"));
        assert_eq!(korean.tries_left(1), "1회 남음");
        assert_eq!(korean.tries_left(3), "3회 남음");
    }

    #[test]
    fn every_localised_string_can_be_drawn() {
        // The app's own fonts have no CJK, so each language's strings are drawn with
        // the subset `scripts/subset-cjk-font.py` builds for it. A glyph missing from
        // it is a row of boxes on the screen.
        use ab_glyph::Font as _;

        for language in crate::assets::languages() {
            let bytes = crate::assets::LANGUAGE_FONTS
                .iter()
                .find(|(shipped, _)| *shipped == language)
                .map(|(_, bytes)| *bytes)
                .unwrap_or_else(|| panic!("`{language}` has no font in `assets::LANGUAGE_FONTS`"));

            let font = ab_glyph::FontVec::try_from_vec(bytes.to_vec())
                .unwrap_or_else(|err| panic!("the `{language}` font does not load: {err}"));

            let name = super::title(Some(language));
            let words = localised_words(Some(language)).map(|(_, word)| word);

            for text in std::iter::once(name).chain(words) {
                for character in text.chars() {
                    if character.is_whitespace() {
                        continue;
                    }
                    assert!(
                        font.glyph_id(character).0 != 0,
                        "the `{language}` font has no glyph for `{character}`, of `{text}` \
                         — re-run scripts/subset-cjk-font.py"
                    );
                }
            }
        }
    }
}
