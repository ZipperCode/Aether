// 身份判定与 Markdown 保护算法参考 MatrixC7/magpie-middleware-antigravity-identity。
// 原实现许可证：
//
// MIT License
// Copyright (c) 2026 MatrixC7
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::{collections::HashMap, sync::LazyLock};

use regex::Regex;

pub(super) const OFFICIAL_ANTIGRAVITY_IDENTITY: &str = "You are Antigravity, a powerful agentic AI coding assistant designed by the Google Deepmind team working on Advanced Agentic Coding.";

const MAX_TEXT_BYTES: usize = 1_000_000;
const MAX_IDENTITY_REPLACEMENTS: usize = 64;
const IDENTITY_SENTENCE_LIMIT: usize = 2_048;
const MAX_OWNER_CHARS: usize = 80;
const MAX_OWNER_WORDS: usize = 6;

const KNOWN_RUNTIMES: &[&str] = &[
    "openclaw",
    "antigravity",
    "codex",
    "claude",
    "cursor",
    "windsurf",
    "aider",
    "cline",
    "kilocode",
    "devin",
    "opencode",
    "dsh",
    "continue",
    "zed",
    "vscode",
    "jetbrains",
    "terminal",
    "cli",
    "editor",
    "ide",
];

const OWNER_STOP_WORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "to",
    "for",
    "with",
    "without",
    "using",
    "use",
    "uses",
    "via",
    "from",
    "of",
    "in",
    "on",
    "at",
    "and",
    "or",
    "but",
    "so",
    "while",
    "if",
    "when",
    "then",
    "that",
    "which",
    "who",
    "whose",
    "it",
    "they",
    "them",
    "you",
    "your",
    "me",
    "us",
    "we",
    "i",
    "must",
    "shall",
    "should",
    "will",
    "would",
    "can",
    "could",
    "may",
    "might",
    "never",
    "always",
    "only",
    "just",
    "also",
    "not",
    "no",
    "do",
    "does",
    "did",
    "please",
    "avoid",
    "ensure",
    "make",
    "keep",
    "remember",
    "note",
    "run",
    "runs",
    "execute",
    "executes",
    "help",
    "helps",
    "helping",
    "created",
    "built",
    "made",
    "developed",
    "designed",
    "trained",
    "powered",
];

struct Matchers {
    billing: Regex,
    role_prefix: Regex,
    non_authorship: Regex,
    model_like: Regex,
    client_prefix: Regex,
    sdk_identity: Regex,
    client_descriptions: Vec<Regex>,
    client_self_descriptions: Vec<Regex>,
    owner_token: Regex,
    owner_model_token: Regex,
    cn_stop_chars: Regex,
    cn_stop_words: Regex,
    cn_identity: Regex,
    cn_model_tail: Regex,
    pure_container: Regex,
    pure_emphasis: Regex,
    list_marker: Regex,
    atx_heading: Regex,
    fence_open: Regex,
    fence_close: Regex,
}

fn matchers() -> &'static Matchers {
    static MATCHERS: LazyLock<Matchers> = LazyLock::new(|| {
        fn re(pattern: &str) -> Regex {
            Regex::new(pattern).expect("静态身份匹配正则必须有效")
        }

        Matchers {
            billing: re(r"(?i)^[ \t]*x-anthropic-billing-header:[^\r\n]*$"),
            role_prefix: re(
                r"(?i)^(?:you[ \t]+are|you're|i[ \t]+am|i'm|act[ \t]+as)[ \t]+(?:an?[ \t]+)?(?:(?:intelligent|advanced|automated|powerful|agentic|coding|software|development|programming|personal|ai)[ \t]+){0,6}(?:agent|assistant|ai|tool|cli|programmer|companion|engineer|bot|harness)[ \t]+((?:(?:created|built|made|developed|designed|trained|powered|maintained|published|released|provided|authored|written)[ \t]+(?:by|on|upon|at))|(?:based[ \t]+on)|(?:running[ \t]+(?:inside|in|within|on))|(?:integrated[ \t]+(?:into|in)))[ \t]+",
            ),
            non_authorship: re(r"(?i)^(?:based|running|integrated)\b"),
            model_like: re(
                r"(?i)^(?:gpt|claude|gemini|llama|qwen|mistral|deepseek|o[1-9])[\w.-]*$",
            ),
            client_prefix: re(
                r"(?i)^(?:you[ \t]+are|you're)[ \t]+(?:dsh|opencode|claude[ \t]+code|codex|hermes[ \t]+agent|openclaw|cursor(?:[ \t]+ai)?|cascade|cline|roo[ \t]+code|aider|kilocode|devin)\b",
            ),
            sdk_identity: re(
                r"(?i)^(?:you[ \t]+are|you're)[ \t]+a[ \t]+claude[ \t]+agent,[ \t]*built[ \t]+on[ \t]+anthropic's[ \t]+claude[ \t]+agent[ \t]+sdk$",
            ),
            client_descriptions: vec![
                re(
                    r"(?i)^,[ \t]*an?[ \t]+interactive[ \t]+cli[ \t]+tool[ \t]+that[ \t]+helps[ \t]+users[ \t]+with[ \t]+software[ \t]+engineering[ \t]+tasks$",
                ),
                re(
                    r"(?i)^,[ \t]*(?:anthropic's[ \t]+)?official[ \t]+cli[ \t]+for[ \t]+claude(?:,[ \t]*running[ \t]+within[ \t]+(?:the[ \t]+)?claude[ \t]+agent[ \t]+sdk)?$",
                ),
                re(
                    r"(?i)^,[ \t]*(?:an?[ \t]+)?(?:advanced[ \t]+)?(?:coding[ \t]+)?agent[ \t]+based[ \t]+on[ \t]+gpt-[a-z0-9._-]+$",
                ),
                re(r"(?i)^,[ \t]*based[ \t]+on[ \t]+gpt-[a-z0-9._-]+$"),
                re(
                    r"(?i)^,[ \t]*an?[ \t]+intelligent[ \t]+ai[ \t]+assistant[ \t]+(?:created|built)[ \t]+by[ \t]+nous[ \t]+research$",
                ),
                re(
                    r"(?i)^,[ \t]*an?[ \t]+automated[ \t]+software[ \t]+development[ \t]+harness(?:[ \t]+designed[ \t]+for[ \t]+agentic[ \t]+coding[ \t]+workflows)?$",
                ),
            ],
            client_self_descriptions: vec![
                re(
                    r"(?i)^(?:the[ \t]+)?codex[ \t]+cli[ \t]+is[ \t]+an?[ \t]+open[ \t-]+source[ \t]+project[ \t]+led[ \t]+by[ \t]+openai$",
                ),
                re(
                    r"(?i)^(?:within[ \t]+this[ \t]+context,[ \t]*)?codex[ \t]+refers[ \t]+to[ \t]+the[ \t]+open[ \t-]+source[ \t]+agentic[ \t]+coding[ \t]+interface(?:[ \t]*\([^)]*\))?$",
                ),
            ],
            owner_token: re(r"^[A-Za-z0-9][A-Za-z0-9'’\-_/]*$"),
            owner_model_token: re(r"^[A-Za-z0-9][A-Za-z0-9'’._/-]*/[A-Za-z0-9][A-Za-z0-9'’._/-]*$"),
            cn_stop_chars: re(
                r"[禁不勿务必请且但并若如注确同只仅需应可允拒执运修改删读写访泄透把将被让使给对从向往于]",
            ),
            cn_stop_words: re(r"(?:为了|因为|作为|认为)"),
            cn_identity: re(
                r"^(?:你是|我是)(?:由)?([^，,。！？\r\n]{1,20}?)(?:开发|制作|构建|训练)的(?:AI)?助手$",
            ),
            cn_model_tail: re(r"^[，,][ \t]*基于[^，,。！？\r\n]{1,40}(?:大模型|模型)$"),
            pure_container: re(r"^[ \t]*(?:[>#]|[-+*][ \t]|[0-9]{1,9}[.)][ \t])"),
            pure_emphasis: re(r"(?s)^[*_]{1,3}.*[*_]{1,3}$"),
            list_marker: re(r"^(?:[-+*]|[0-9]{1,9}[.)])([ \t]+)"),
            atx_heading: re(r"^#{1,6}(?:[ \t]|$)"),
            fence_open: re(r"^(`{3,}|~{3,})(.*)$"),
            fence_close: re(r"^(`{3,}|~{3,})[ \t]*$"),
        }
    });
    &MATCHERS
}

fn official_identity_stem() -> &'static str {
    OFFICIAL_ANTIGRAVITY_IDENTITY
        .strip_suffix('.')
        .expect("官方身份必须以句号结尾")
}

fn identity_replacement(punctuation: Option<char>) -> String {
    match punctuation {
        Some(',' | '，') => {
            let mut result = String::from(official_identity_stem());
            result.push(punctuation.expect("已匹配标点"));
            result
        }
        _ => String::from(OFFICIAL_ANTIGRAVITY_IDENTITY),
    }
}

/// 只接受简单归属名；含有约束、目的或复合结构的文本一律保留。
fn is_simple_owner(owner: &str) -> bool {
    if owner.is_empty() || owner.chars().count() > MAX_OWNER_CHARS {
        return false;
    }
    let words: Vec<&str> = owner
        .split(|character| character == ' ' || character == '\t')
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() || words.len() > MAX_OWNER_WORDS {
        return false;
    }

    let matchers = matchers();
    for (index, word) in words.iter().enumerate() {
        if !matchers.owner_token.is_match(word) && !matchers.owner_model_token.is_match(word) {
            return false;
        }
        let lowercase = word.to_lowercase();
        if index == 0 && matches!(lowercase.as_str(), "a" | "an" | "the") {
            continue;
        }
        if OWNER_STOP_WORDS.contains(&lowercase.as_str()) {
            return false;
        }
    }
    true
}

fn is_simple_chinese_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.chars().count() <= 20
        && !matchers().cn_stop_words.is_match(owner)
        && !matchers().cn_stop_chars.is_match(owner)
}

/// 点号后紧跟非空白时不视为句界，避免截断域名、路径和版本号。
fn sentence_end(text: &str, start: usize) -> usize {
    for (offset, character) in text[start..].char_indices() {
        let index = start + offset;
        if character == '.' {
            if let Some(next) = text[index + 1..].chars().next() {
                if next != ' ' && next != '\t' {
                    continue;
                }
            }
            return index;
        }
        if matches!(character, '。' | '!' | '?' | '！' | '？' | ';' | '；') {
            return index;
        }
    }
    text.len()
}

fn append_punctuation(output: &mut String, punctuation: Option<char>) {
    if let Some(punctuation) = punctuation {
        output.push(punctuation);
    }
}

/// 仅识别参考实现明确列出的身份句，无法确定时返回 None。
fn match_identity(sentence: &str, punctuation: Option<char>) -> Option<String> {
    if matches!(punctuation, Some('?' | '？')) {
        return None;
    }

    let matchers = matchers();
    if let Some(known) = matchers.client_prefix.find(sentence) {
        let tail = &sentence[known.end()..];
        if tail.is_empty()
            || matchers
                .client_descriptions
                .iter()
                .any(|description| description.is_match(tail))
        {
            return Some(String::from(OFFICIAL_ANTIGRAVITY_IDENTITY));
        }
        return None;
    }

    if matchers.sdk_identity.is_match(sentence)
        || matchers
            .client_self_descriptions
            .iter()
            .any(|description| description.is_match(sentence))
    {
        return Some(String::from(OFFICIAL_ANTIGRAVITY_IDENTITY));
    }

    let comma = sentence
        .char_indices()
        .find_map(|(index, character)| matches!(character, ',' | '，').then_some(index));
    let (head, tail) = match comma {
        Some(index) => (&sentence[..index], &sentence[index..]),
        None => (sentence, ""),
    };

    if let Some(captures) = matchers.role_prefix.captures(head) {
        let prefix = captures.get(0).expect("完整匹配必须存在");
        let clause = captures.get(1).expect("归属从句必须存在").as_str();
        let owner = head[prefix.end()..].trim();
        if !is_simple_owner(owner) {
            return None;
        }
        if matchers.non_authorship.is_match(clause) {
            let first = owner
                .split(|character| character == ' ' || character == '\t')
                .next()
                .unwrap_or_default()
                .to_lowercase();
            if !KNOWN_RUNTIMES.contains(&first.as_str()) && !matchers.model_like.is_match(&first) {
                return None;
            }
        }
        if tail.is_empty() {
            return Some(identity_replacement(punctuation));
        }

        let comma = tail.chars().next().expect("尾句必须以逗号开头");
        let mut result = identity_replacement(Some(comma));
        result.push_str(&tail[comma.len_utf8()..]);
        append_punctuation(&mut result, punctuation);
        return Some(result);
    }

    if let Some(captures) = matchers.cn_identity.captures(head) {
        let owner = captures.get(1).expect("中文归属名必须存在").as_str();
        if !is_simple_chinese_owner(owner) {
            return None;
        }
        if tail.is_empty() || matchers.cn_model_tail.is_match(tail) {
            return Some(String::from(OFFICIAL_ANTIGRAVITY_IDENTITY));
        }
        let comma = tail.chars().next().expect("尾句必须以逗号开头");
        let mut result = identity_replacement(Some(comma));
        result.push_str(&tail[comma.len_utf8()..]);
        append_punctuation(&mut result, punctuation);
        return Some(result);
    }

    None
}

/// 穿透引用、列表和标题容器，但只消费合法且有限的行首标记。
fn skip_containers(line: &str) -> usize {
    let mut at = 0;
    for _ in 0..16 {
        let bytes = line.as_bytes();
        let mut cursor = at;
        let mut spaces = 0;
        while cursor < bytes.len() && bytes[cursor] == b' ' && spaces < 3 {
            cursor += 1;
            spaces += 1;
        }
        if cursor >= bytes.len() {
            return at;
        }

        if bytes[cursor] == b'>' {
            cursor += 1;
            if cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t') {
                cursor += 1;
            }
            at = cursor;
            continue;
        }

        let rest = &line[cursor..];
        if let Some(marker) = matchers().list_marker.find(rest) {
            at = cursor + marker.end();
            continue;
        }
        if let Some(heading) = matchers().atx_heading.find(rest) {
            at = cursor + heading.end();
            continue;
        }
        return at;
    }
    at
}

fn unwrap_emphasis(content: &str) -> Option<(&str, &str)> {
    for wrapper in ["***", "**", "*", "___", "__", "_"] {
        if content.len() >= wrapper.len() * 2
            && content.starts_with(wrapper)
            && content.ends_with(wrapper)
        {
            let inner = &content[wrapper.len()..content.len() - wrapper.len()];
            if !inner.trim().is_empty() {
                return Some((wrapper, inner));
            }
        }
    }
    None
}

fn process_line(line: &str, skip_identity: bool, partial_tail: bool) -> Option<String> {
    if !partial_tail && !line.contains('`') && matchers().billing.is_match(line) {
        return None;
    }

    let base = skip_containers(line);
    let prefix = &line[..base];
    let (wrapper, content) = unwrap_emphasis(&line[base..]).unwrap_or(("", &line[base..]));
    let mut output = String::with_capacity(line.len());
    let mut position = 0;
    let mut first_sentence = true;
    let mut replacements = 0;

    loop {
        let mut start = position;
        while start < content.len() && matches!(content.as_bytes()[start], b' ' | b'\t') {
            start += 1;
        }
        if start >= content.len() {
            output.push_str(&content[position..]);
            break;
        }

        let end = sentence_end(content, start);
        let sentence = &content[start..end];
        let punctuation =
            (end < content.len()).then(|| content[end..].chars().next().expect("句界处必须有字符"));
        let at_end = end >= content.len();
        let skip =
            (skip_identity && first_sentence) || (partial_tail && at_end && punctuation.is_none());

        let replacement = if !skip
            && sentence.chars().count() <= IDENTITY_SENTENCE_LIMIT
            && replacements < MAX_IDENTITY_REPLACEMENTS
        {
            match_identity(sentence, punctuation)
        } else {
            None
        };
        if replacement.is_some() {
            replacements += 1;
        }

        output.push_str(&content[position..start]);
        if let Some(replacement) = replacement {
            output.push_str(&replacement);
        } else {
            output.push_str(sentence);
            append_punctuation(&mut output, punctuation);
        }

        first_sentence = false;
        if at_end {
            break;
        }
        position = end + punctuation.expect("非末尾句界必须有标点").len_utf8();
    }

    let mut result = String::with_capacity(prefix.len() + output.len() + wrapper.len() * 2);
    result.push_str(prefix);
    result.push_str(wrapper);
    result.push_str(&output);
    result.push_str(wrapper);
    Some(result)
}

#[derive(Clone, Copy)]
struct CodeRun {
    start: usize,
    end: usize,
    len: usize,
}

fn is_escaped(text: &str, index: usize) -> bool {
    let bytes = text.as_bytes();
    let mut cursor = index;
    let mut slashes = 0;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        slashes += 1;
        cursor -= 1;
    }
    slashes % 2 == 1
}

/// 预计算下一同长征程，避免大量反引号时的二次扫描。
fn find_code_spans(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut runs = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'`' {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor] == b'`' {
            cursor += 1;
        }
        runs.push(CodeRun {
            start,
            end: cursor,
            len: cursor - start,
        });
    }

    let mut next_same = vec![None; runs.len()];
    let mut next_by_len = HashMap::<usize, usize>::new();
    for index in (0..runs.len()).rev() {
        next_same[index] = next_by_len.get(&runs[index].len).copied();
        next_by_len.insert(runs[index].len, index);
    }

    let mut spans = Vec::new();
    let mut index = 0;
    while index < runs.len() {
        if is_escaped(text, runs[index].start) {
            index += 1;
            continue;
        }
        let Some(mate) = next_same[index] else {
            index += 1;
            continue;
        };
        spans.push((runs[index].start, runs[mate].end));
        index = mate + 1;
    }
    spans
}

fn split_line_terminator(raw: &str) -> (&str, &str) {
    if let Some(body) = raw.strip_suffix("\r\n") {
        (body, "\r\n")
    } else if let Some(body) = raw.strip_suffix('\n') {
        (body, "\n")
    } else {
        (raw, "")
    }
}

fn sanitize_fragment(fragment: &str, starts_after_code: bool, ends_before_code: bool) -> String {
    if fragment.is_empty() {
        return String::new();
    }

    let mut kept = String::with_capacity(fragment.len());
    let mut at = 0;
    let mut index = 0;
    loop {
        let newline = fragment[at..].find('\n').map(|offset| at + offset);
        let end = newline.map_or(fragment.len(), |index| index + 1);
        let (body, terminator) = split_line_terminator(&fragment[at..end]);
        let is_first = index == 0;
        let is_last = newline.is_none();
        if let Some(line) = process_line(
            body,
            starts_after_code && is_first,
            ends_before_code && is_last,
        ) {
            kept.push_str(&line);
            kept.push_str(terminator);
        }
        index += 1;
        at = end;
        if is_last {
            break;
        }
    }
    kept
}

/// 行内代码按实体内容原样复制，身份匹配只作用于其两侧的散文。
fn sanitize_paragraph(text: &str) -> String {
    let spans = find_code_spans(text);
    if spans.is_empty() {
        return sanitize_fragment(text, false, false);
    }

    let mut output = String::with_capacity(text.len());
    let mut at = 0;
    let mut after_code = false;
    for (start, end) in spans {
        output.push_str(&sanitize_fragment(&text[at..start], after_code, true));
        output.push_str(&text[start..end]);
        at = end;
        after_code = true;
    }
    output.push_str(&sanitize_fragment(&text[at..], after_code, false));
    output
}

fn is_pure_identity_paragraph(paragraph: &str) -> bool {
    let body = paragraph.trim();
    if body.is_empty()
        || body.chars().count() > IDENTITY_SENTENCE_LIMIT
        || body.contains(['\n', '\r', '`'])
        || matchers().pure_container.is_match(body)
        || matchers().pure_emphasis.is_match(body)
    {
        return false;
    }

    let end = sentence_end(body, 0);
    let punctuation = if end < body.len() {
        let punctuation = body[end..].chars().next().expect("句界处必须有字符");
        if end + punctuation.len_utf8() != body.len() {
            return false;
        }
        Some(punctuation)
    } else {
        None
    };
    if !matches!(punctuation, None | Some('.' | '。' | '!' | '！')) {
        return false;
    }

    let sentence = &body[..end];
    if sentence == official_identity_stem() && matches!(punctuation, None | Some('.')) {
        return true;
    }
    match_identity(sentence, punctuation).as_deref() == Some(OFFICIAL_ANTIGRAVITY_IDENTITY)
}

fn advance_column(character: u8, column: isize) -> isize {
    if character == b'\t' {
        column + (4 - column.rem_euclid(4))
    } else {
        column + 1
    }
}

#[derive(Debug)]
struct LineInfo {
    indent_cols: isize,
    blank: bool,
    content_start: usize,
    list_marker: bool,
    content_indent: isize,
}

fn analyze_line(body: &str) -> LineInfo {
    let bytes = body.as_bytes();
    let mut index = 0;
    let mut column = 0;

    // 引用前缀可嵌套，每层的可选前导空格最多三个。
    loop {
        let mut cursor = index;
        let mut current_column = column;
        let mut spaces = 0;
        while cursor < bytes.len() && bytes[cursor] == b' ' && spaces < 3 {
            cursor += 1;
            current_column += 1;
            spaces += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b'>' {
            cursor += 1;
            current_column += 1;
            if cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t') {
                current_column = advance_column(bytes[cursor], current_column);
                cursor += 1;
            }
            index = cursor;
            column = current_column;
            continue;
        }
        break;
    }

    let mut cursor = index;
    let mut current_column = column;
    while cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t') {
        current_column = advance_column(bytes[cursor], current_column);
        cursor += 1;
    }
    let indent_cols = current_column - column;
    if cursor >= bytes.len() {
        return LineInfo {
            indent_cols,
            blank: true,
            content_start: cursor,
            list_marker: false,
            content_indent: 0,
        };
    }

    let mut info = LineInfo {
        indent_cols,
        blank: false,
        content_start: cursor,
        list_marker: false,
        content_indent: 0,
    };
    if let Some(marker) = matchers().list_marker.find(&body[cursor..]) {
        info.list_marker = true;
        info.content_start = cursor + marker.end();
        info.content_indent = current_column + marker.len() as isize;
    }
    info
}

#[derive(Clone, Copy)]
struct Fence {
    character: u8,
    len: usize,
}

fn fence_opener(text: &str) -> Option<Fence> {
    let captures = matchers().fence_open.captures(text)?;
    let marks = captures.get(1)?.as_str();
    let info = captures.get(2)?.as_str();
    let character = marks.as_bytes()[0];
    if character == b'`' && info.contains('`') {
        return None;
    }
    Some(Fence {
        character,
        len: marks.len(),
    })
}

fn fence_closer(text: &str, fence: Fence) -> bool {
    let Some(captures) = matchers().fence_close.captures(text) else {
        return false;
    };
    let marks = captures.get(1).expect("围栏标记必须存在").as_str();
    marks.as_bytes()[0] == fence.character && marks.len() >= fence.len
}

/// 文首窗口内仅折叠完整的顶层纯身份段；其他内容会永久关闭窗口。
fn flush_prose(
    prose: &mut String,
    output: &mut Vec<String>,
    prefix_open: &mut bool,
    identity_emitted: &mut bool,
    suppress_blank: &mut bool,
) {
    if prose.is_empty() {
        return;
    }
    let raw = std::mem::take(prose);
    let text = sanitize_paragraph(&raw);
    if text.is_empty() {
        *suppress_blank = true;
        return;
    }

    // 必须基于清洗后的段落判定，保证重复执行得到同一结果。
    let pure = is_pure_identity_paragraph(&text);
    if *prefix_open && pure && *identity_emitted {
        *suppress_blank = true;
        return;
    }
    if !pure {
        *prefix_open = false;
    } else if *prefix_open {
        *identity_emitted = true;
    }
    output.push(text);
    *suppress_blank = false;
}

/// 清理单个系统文本 part。调用方必须为每个 part 单独调用，不能共享去重状态。
pub(super) fn sanitize_system_text(text: &str) -> String {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES {
        return String::from(text);
    }

    let mut output = Vec::<String>::new();
    let mut prose = String::new();
    let mut fence = None;
    let mut item_indent = 0_isize;
    let mut paragraph_open = false;
    let mut prefix_open = true;
    let mut identity_emitted = false;
    let mut suppress_blank = false;

    let mut at = 0;
    while at < text.len() {
        let newline = text[at..].find('\n').map(|offset| at + offset);
        let end = newline.map_or(text.len(), |index| index + 1);
        let line = &text[at..end];
        at = end;
        let (body, _) = split_line_terminator(line);
        let info = analyze_line(body);

        if item_indent > 0 && !info.blank && !info.list_marker && info.indent_cols < item_indent {
            item_indent = 0;
        }
        let effective_indent = info.indent_cols - item_indent;

        if let Some(active_fence) = fence {
            flush_prose(
                &mut prose,
                &mut output,
                &mut prefix_open,
                &mut identity_emitted,
                &mut suppress_blank,
            );
            output.push(String::from(line));
            if effective_indent <= 3 && fence_closer(&body[info.content_start..], active_fence) {
                fence = None;
            }
            paragraph_open = false;
            prefix_open = false;
            suppress_blank = false;
            continue;
        }

        if info.blank {
            flush_prose(
                &mut prose,
                &mut output,
                &mut prefix_open,
                &mut identity_emitted,
                &mut suppress_blank,
            );
            if suppress_blank {
                paragraph_open = false;
                continue;
            }
            output.push(String::from(line));
            paragraph_open = false;
            continue;
        }

        suppress_blank = false;

        // 缩进代码不能打断已有段落，否则约束重排后可能改变代码块语义。
        if effective_indent >= 4 && !paragraph_open {
            flush_prose(
                &mut prose,
                &mut output,
                &mut prefix_open,
                &mut identity_emitted,
                &mut suppress_blank,
            );
            output.push(String::from(line));
            prefix_open = false;
            continue;
        }

        if info.list_marker {
            flush_prose(
                &mut prose,
                &mut output,
                &mut prefix_open,
                &mut identity_emitted,
                &mut suppress_blank,
            );
            item_indent = info.content_indent;
            if let Some(opener) = fence_opener(&body[info.content_start..]) {
                output.push(String::from(line));
                fence = Some(opener);
                paragraph_open = false;
                prefix_open = false;
                continue;
            }
            prose.push_str(line);
            paragraph_open = true;
            continue;
        }

        if effective_indent <= 3 {
            if let Some(opener) = fence_opener(&body[info.content_start..]) {
                flush_prose(
                    &mut prose,
                    &mut output,
                    &mut prefix_open,
                    &mut identity_emitted,
                    &mut suppress_blank,
                );
                output.push(String::from(line));
                fence = Some(opener);
                paragraph_open = false;
                prefix_open = false;
                continue;
            }
        }

        if effective_indent <= 3 && matchers().atx_heading.is_match(&body[info.content_start..]) {
            flush_prose(
                &mut prose,
                &mut output,
                &mut prefix_open,
                &mut identity_emitted,
                &mut suppress_blank,
            );
            prose.push_str(line);
            flush_prose(
                &mut prose,
                &mut output,
                &mut prefix_open,
                &mut identity_emitted,
                &mut suppress_blank,
            );
            paragraph_open = false;
            prefix_open = false;
            continue;
        }

        prose.push_str(line);
        paragraph_open = true;
    }
    flush_prose(
        &mut prose,
        &mut output,
        &mut prefix_open,
        &mut identity_emitted,
        &mut suppress_blank,
    );

    // 末段被丢弃时，只回退此前独立的空行，避免留下悬挂空行。
    if suppress_blank {
        while matches!(output.last().map(String::as_str), Some("\n" | "\r\n")) {
            output.pop();
        }
    }

    let mut result = output.concat();
    if !matches!(text.as_bytes().last(), Some(b'\n' | b'\r')) {
        while matches!(result.as_bytes().last(), Some(b'\n' | b'\r')) {
            result.pop();
        }
    }
    result
}
