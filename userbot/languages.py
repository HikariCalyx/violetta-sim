"""The verification message in the recipient's Telegram language.

The userbot sends one photo with this text. Telegram reports the recipient's client
language on the ``User`` object (``lang_code``), so the first contact is greeted in the
language that client is actually using. The character names match the game's own
localisations (``碧欧蕾塔`` / ``菲歐娜`` / ``ビオルタ`` / ``비올레타``).
"""

from __future__ import annotations

MESSAGES: dict[str, str] = {
    "en": (
        "Princess Violetta told me that you must pass her test or marry her before you can talk to me.\n"
        "\n"
        "I guess you don't want to marry her right?\n"
        "To pass her test, click following link:\n"
        "{url}"
    ),
    "cn": (
        "碧欧蕾塔公主告诉我，你必须通过她的考验，或者娶她，才能和我说话。\n"
        "\n"
        "我猜你不想娶她吧？\n"
        "要通过她的考验，请点击以下链接：\n"
        "{url}"
    ),
    "tw": (
        "菲歐娜公主告訴我，你必須通過她的考驗，或者娶她，才能和我說話。\n"
        "\n"
        "我猜你不想娶她吧？\n"
        "要通過她的考驗，請點擊以下連結：\n"
        "{url}"
    ),
    "jp": (
        "ビオルタ姫から、私と話すには彼女の試練に合格するか、彼女と結婚しなければならないと聞きました。\n"
        "\n"
        "結婚したくはないですよね？\n"
        "試練に合格するには、次のリンクをクリックしてください：\n"
        "{url}"
    ),
    "kr": (
        "비올레타 공주님이 저와 대화하려면 그녀의 시험을 통과하거나 그녀와 결혼해야 한다고 하셨어요.\n"
        "\n"
        "결혼하고 싶지는 않으시죠?\n"
        "시험을 통과하려면 다음 링크를 클릭하세요:\n"
        "{url}"
    ),
}


def preferred_language(lang_code: str | None) -> str:
    """Map a Telegram ``lang_code`` to one of the shipped message languages.

    Telegram usually sends ``zh-hans`` / ``zh-hant`` for Chinese, but some clients send
    ``zh-cn`` / ``zh-tw``; an explicit script wins over a region, and a bare ``zh``
    falls back to Simplified. Everything that is not Chinese, Japanese or Korean uses
    the English message.
    """
    tag = (lang_code or "").strip().lower().replace("_", "-")
    if tag.startswith("zh"):
        parts = set(tag.split("-"))
        if "hans" in tag or "cn" in parts or "sg" in parts:
            return "cn"
        if "hant" in tag or parts & {"tw", "hk", "mo"}:
            return "tw"
        return "cn"
    if tag.startswith("ja"):
        return "jp"
    if tag.startswith("ko"):
        return "kr"
    return "en"


def verification_message(lang_code: str | None, url: str) -> str:
    return MESSAGES[preferred_language(lang_code)].format(url=url)
