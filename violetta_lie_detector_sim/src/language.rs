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
/// the buttons, and how the result screen reports the outcome.
///
/// These are what the game's own wording is for, so they are short: a caption
/// over an outcome, and the three buttons.
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
}

impl Words {
    /// A language's words, spelled out as the app itself writes them.
    const fn new(
        start: &'static str,
        chaos: Option<&'static str>,
        try_again: &'static str,
        success: &'static str,
        failed: &'static str,
    ) -> Self {
        Self {
            start,
            chaos,
            try_again,
            success,
            failed,
        }
    }
}

/// The words the app writes in `language`.
///
/// One language to a line, like [`title`] and for the same reason:
/// `scripts/subset-cjk-font.py` reads them from this file to know which glyphs the
/// embedded fonts have to hold. English is the base artwork and the language the
/// app is written in, so it is what everything else falls back to.
///
/// Returned by value rather than as a `&'static`, because a reference to a
/// constructor call is not a promoted constant: it would have to be built from a
/// `const` item per language, which is one table too many.
pub fn words(language: Option<&'static str>) -> Words {
    match language {
        Some("cn") => Words::new("开始", Some("进阶模式"), "再试一次", "成功", "失败"),
        Some("jp") => Words::new("スタート", Some("カオスモード"), "もう一度", "成功", "失敗"),
        Some("kr") => Words::new("시작", Some("카오스 모드"), "다시하기", "성공", "실패"),
        Some("tw") => Words::new("開始", Some("混沌模式"), "再試一次", "成功", "失敗"),
        _ => Words::new("Start", None, "Try Again", "Success", "Failed"),
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

    /// Everything the app writes for itself in `language`: its name, and then the
    /// words the screens are captioned and buttoned with.
    fn localised_strings(language: &'static str) -> Vec<&'static str> {
        let words = super::words(Some(language));
        let mut strings = vec![
            super::title(Some(language)),
            words.start,
            words.try_again,
            words.success,
            words.failed,
        ];
        // A language that has not named the mode draws the English name instead.
        strings.extend(words.chaos);
        strings
    }

    #[test]
    fn every_shipped_language_has_strings_of_its_own() {
        let english = super::words(None);

        for language in crate::assets::languages() {
            let words = super::words(Some(language));
            for (word, localised, base) in [
                ("start", words.start, english.start),
                ("try_again", words.try_again, english.try_again),
                ("success", words.success, english.success),
                ("failed", words.failed, english.failed),
            ] {
                assert_ne!(
                    localised, base,
                    "`{language}`'s `{word}` is still the English one"
                );
            }

            if let Some(chaos) = words.chaos {
                assert_ne!(
                    chaos, "Chaos Mode",
                    "`{language}`'s `chaos` is still the English one"
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

            for text in localised_strings(language) {
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
