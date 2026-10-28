//! Vertex `functionCall.partialArgs` 增量组装。
//!
//! Gemini/Vertex 流式工具调用会把参数拆成带 `jsonPath` 的片段：字符串可分片延续
//! （`willContinue`），对象键与数组下标逐叶到达。本模块把片段组装为参数 JSON，
//! 并且只在能安全追加时立即输出增量文本，否则把剩余部分缓冲到调用终态一次性补发。
//!
//! 不变量（改动前请确认）：
//! - 已输出文本永远是最终参数 JSON 的“未闭合前缀”：流中任意时刻都不构成完整 JSON，
//!   终态补发剩余后缀后才闭合；因此调用方在终态前不会误判为完整成功参数。
//! - 只有目标容器路径位于已输出路径（`open_frame`）之下时才能追加；需要回退到已离开
//!   容器的新叶子无法再插入文本，只能缓冲到终态。
//! - 一旦有任何叶子被缓冲，后续叶子一律不再输出：已输出前缀必须覆盖 DFS 顺序里所有
//!   更早的叶子，否则补发时无法把中间叶子插回文本。
//! - 同一路径只允许“未闭合字符串续写”或完全相同的值；其他重复写入是类型/取值冲突。
//! - 数组只允许按当前长度追加或续写已有元素；稀疏下标与越界下标一律拒绝。
//! - 终态必须没有未闭合字符串，否则视为被截断而失败，绝不补成成功调用。

use std::collections::HashSet;

use serde_json::{Map, Value};

/// 路径段数量上限：防止恶意深层 jsonPath 造成内存放大。
const MAX_PARTIAL_ARG_PATH_SEGMENTS: usize = 64;
/// 数组下标上限：防止恶意下标强制分配巨大数组。
const MAX_PARTIAL_ARG_ARRAY_INDEX: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ArgsPathSegment {
    Key(String),
    Index(usize),
}

type ArgsPath = Vec<ArgsPathSegment>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeafWrite {
    Created,
    Appended,
    Unchanged,
}

enum PartialArgValue {
    String(String),
    Scalar(Value),
}

/// 单个流式工具调用的分片参数组装状态。
pub(crate) struct VertexPartialArgsAssembler {
    root: Map<String, Value>,
    /// 已输出的未闭合 JSON 前缀。
    emitted: String,
    /// `emitted` 中当前打开的容器路径（根为空路径）。
    open_frame: ArgsPath,
    /// 每个打开容器是否已写入成员，与 `open_frame` 一一对应。
    frame_members: Vec<bool>,
    /// 已输出的仍在延续的字符串叶子；只有它能继续追加文本。
    emitted_open_string: Option<ArgsPath>,
    /// 所有仍在延续的字符串叶子（含被缓冲、尚未输出的）。
    open_strings: HashSet<ArgsPath>,
    /// 首个无法安全追加的叶子出现后置位，后续不再输出任何增量。
    buffering: bool,
    finished: bool,
}

impl Default for VertexPartialArgsAssembler {
    fn default() -> Self {
        Self {
            // 根容器始终存在一个未写成员的帧，保证分隔符判断有基准。
            frame_members: vec![false],
            root: Map::new(),
            emitted: String::new(),
            open_frame: Vec::new(),
            emitted_open_string: None,
            open_strings: HashSet::new(),
            buffering: false,
            finished: false,
        }
    }
}

impl VertexPartialArgsAssembler {
    /// 是否仍有未闭合字符串（调用在分片语义上尚未结束）。
    pub(crate) fn has_open_strings(&self) -> bool {
        !self.open_strings.is_empty()
    }

    /// 应用一个 functionCall 分片里的全部 partialArgs 条目，返回可立即输出的参数增量。
    pub(crate) fn push_entries(&mut self, entries: &[Value]) -> Result<String, String> {
        if self.finished {
            return Err("Gemini 函数调用分片参数在终态之后仍收到增量".to_string());
        }
        let mut delta = String::new();
        for entry in entries {
            delta.push_str(&self.apply_entry(entry)?);
        }
        Ok(delta)
    }

    /// 终结调用：校验没有未闭合字符串，并返回补齐闭合结构的剩余增量。
    pub(crate) fn finish(&mut self) -> Result<String, String> {
        if self.finished {
            return Ok(String::new());
        }
        if !self.open_strings.is_empty() {
            return Err("Gemini 函数调用分片参数在终态仍存在未闭合字符串".to_string());
        }
        debug_assert!(self.emitted_open_string.is_none());
        self.finished = true;
        let full = serde_json::to_string(&self.root)
            .map_err(|error| format!("Gemini 函数调用分片参数序列化失败: {error}"))?;
        if !full.starts_with(self.emitted.as_str()) {
            // 只会出现在组装器自身顺序错误时；宁可失败也不输出不一致的参数。
            return Err("Gemini 函数调用分片参数无法与已输出前缀对齐".to_string());
        }
        let suffix = full[self.emitted.len()..].to_string();
        self.emitted = full;
        Ok(suffix)
    }

    fn apply_entry(&mut self, entry: &Value) -> Result<String, String> {
        let object = entry
            .as_object()
            .ok_or_else(|| "Gemini partialArgs 条目必须是对象".to_string())?;
        let raw_path = object
            .get("jsonPath")
            .or_else(|| object.get("json_path"))
            .and_then(Value::as_str)
            .ok_or_else(|| "Gemini partialArgs 条目缺少字符串 jsonPath".to_string())?;
        let path = parse_args_path(raw_path)?;
        let (value, will_continue) = parse_entry_value(object, raw_path)?;

        match value {
            None => {
                // 无值条目是叶子终结标记：默认收束仍在延续的字符串。
                if will_continue == Some(true) {
                    if !self.open_strings.contains(&path) {
                        return Err(format!(
                            "Gemini partialArgs 延续标记 {raw_path} 没有对应的未闭合字符串"
                        ));
                    }
                    return Ok(String::new());
                }
                self.close_string_leaf(&path, raw_path)
            }
            Some(PartialArgValue::String(fragment)) => {
                let was_open = self.open_strings.contains(&path);
                if !was_open {
                    if let Some(existing) = self.lookup(&path) {
                        return Err(match existing {
                            Value::String(_) => {
                                format!("Gemini partialArgs 字符串 {raw_path} 已结束却继续分片")
                            }
                            Value::Object(_) | Value::Array(_) => {
                                format!("Gemini partialArgs 路径 {raw_path} 与已组装参数类型冲突")
                            }
                            _ => format!("Gemini partialArgs 路径 {raw_path} 已存在非字符串取值"),
                        });
                    }
                }
                let write = self.write_leaf(&path, &PartialArgValue::String(fragment.clone()))?;
                if write == LeafWrite::Unchanged {
                    return Ok(String::new());
                }
                self.apply_string_emission(&path, fragment, will_continue, write)
            }
            Some(PartialArgValue::Scalar(value)) => {
                if will_continue == Some(true) {
                    return Err(format!(
                        "Gemini partialArgs 只有字符串叶子可以 willContinue，路径 {raw_path}"
                    ));
                }
                let write = self.write_leaf(&path, &PartialArgValue::Scalar(value.clone()))?;
                if write != LeafWrite::Created {
                    return Ok(String::new());
                }
                Ok(self.emit_created_leaf(&path, &PartialArgValue::Scalar(value)))
            }
        }
    }

    fn close_string_leaf(&mut self, path: &ArgsPath, raw_path: &str) -> Result<String, String> {
        if !self.open_strings.remove(path) {
            if self.lookup(path).is_some() {
                return Ok(String::new());
            }
            return Err(format!(
                "Gemini partialArgs 终结标记 {raw_path} 指向不存在的参数路径"
            ));
        }
        if self.emitted_open_string.as_ref() != Some(path) {
            return Ok(String::new());
        }
        self.emitted_open_string = None;
        self.emitted.push('"');
        Ok("\"".to_string())
    }

    fn apply_string_emission(
        &mut self,
        path: &ArgsPath,
        fragment: String,
        will_continue: Option<bool>,
        write: LeafWrite,
    ) -> Result<String, String> {
        let open_after = will_continue == Some(true);
        if self.emitted_open_string.as_ref() == Some(path) {
            let mut delta = escape_fragment(&fragment);
            self.emitted.push_str(&delta);
            if open_after {
                self.open_strings.insert(path.clone());
            } else {
                self.open_strings.remove(path);
                self.emitted_open_string = None;
                self.emitted.push('"');
                delta.push('"');
            }
            return Ok(delta);
        }
        if open_after {
            self.open_strings.insert(path.clone());
        } else {
            self.open_strings.remove(path);
        }
        if write != LeafWrite::Created || self.emitted_open_string.is_some() {
            // 续写已被缓冲的字符串，或已有另一条未闭合字符串占据输出尾部。
            self.buffering = true;
            return Ok(String::new());
        }
        Ok(self.emit_created_leaf(path, &PartialArgValue::String(fragment)))
    }

    /// 输出一个刚创建的叶子；不安全时进入缓冲模式且本次不输出。
    fn emit_created_leaf(&mut self, path: &ArgsPath, value: &PartialArgValue) -> String {
        if self.buffering {
            return String::new();
        }
        let parent = &path[..path.len() - 1];
        if !parent.starts_with(self.open_frame.as_slice()) {
            // 需要回退到已离开的容器：后续叶子无法再插入文本。
            self.buffering = true;
            return String::new();
        }
        let only_this_string = self.open_strings.is_empty()
            || (self.open_strings.len() == 1 && self.open_strings.contains(path));
        if !only_this_string {
            self.buffering = true;
            return String::new();
        }
        let open_string =
            matches!(value, PartialArgValue::String(_)) && self.open_strings.contains(path);
        let text = value_text(value, open_string);
        let start = self.emitted.len();
        self.emit_leaf_text(path, &text);
        if open_string {
            self.emitted_open_string = Some(path.clone());
        }
        // 返回本次真正追加的文本（含根/容器的开括号、键与分隔符），而不是单个值。
        self.emitted[start..].to_string()
    }

    fn emit_leaf_text(&mut self, path: &ArgsPath, value_text: &str) {
        // 参数根固定是对象；首次输出时补上根容器的 `{`（根帧本身没有父键可触发）。
        if self.emitted.is_empty() {
            self.emitted.push('{');
        }
        let parent_len = path.len() - 1;
        debug_assert!(self.open_frame.len() <= parent_len);
        debug_assert!(path[..parent_len].starts_with(self.open_frame.as_slice()));
        for depth in self.open_frame.len()..parent_len {
            if *self.frame_members.last().expect("root frame") {
                self.emitted.push(',');
            }
            if let ArgsPathSegment::Key(key) = &path[depth] {
                push_quoted(&mut self.emitted, key);
                self.emitted.push(':');
            }
            let child_is_object = matches!(path.get(depth + 1), Some(ArgsPathSegment::Key(_)));
            self.emitted.push(if child_is_object { '{' } else { '[' });
            *self.frame_members.last_mut().expect("root frame") = true;
            self.frame_members.push(false);
            self.open_frame.push(path[depth].clone());
        }
        if *self.frame_members.last().expect("root frame") {
            self.emitted.push(',');
        }
        if let ArgsPathSegment::Key(key) = path.last().expect("leaf segment") {
            push_quoted(&mut self.emitted, key);
            self.emitted.push(':');
        }
        self.emitted.push_str(value_text);
        *self.frame_members.last_mut().expect("root frame") = true;
    }

    fn lookup(&self, path: &ArgsPath) -> Option<&Value> {
        let (first, rest) = path.split_first()?;
        let ArgsPathSegment::Key(key) = first else {
            // 参数根必须是对象，根级下标不可能存在。
            return None;
        };
        let mut current = self.root.get(key)?;
        for segment in rest {
            current = match segment {
                ArgsPathSegment::Key(key) => current.as_object()?.get(key)?,
                ArgsPathSegment::Index(index) => current.as_array()?.get(*index)?,
            };
        }
        Some(current)
    }

    fn write_leaf(
        &mut self,
        path: &ArgsPath,
        value: &PartialArgValue,
    ) -> Result<LeafWrite, String> {
        // 借用期间把根移到临时 Value 中导航，结束后无条件归还，避免整树复制。
        let mut holder = Value::Object(std::mem::take(&mut self.root));
        let result = write_into_leaf(&mut holder, path, value);
        let Value::Object(map) = holder else {
            unreachable!("partialArgs root holder must stay an object");
        };
        self.root = map;
        result
    }
}

fn write_into_leaf(
    root: &mut Value,
    path: &ArgsPath,
    value: &PartialArgValue,
) -> Result<LeafWrite, String> {
    let (leaf, parents) = path
        .split_last()
        .ok_or_else(|| "Gemini partialArgs 路径为空".to_string())?;
    let mut container = root;
    for (position, segment) in parents.iter().enumerate() {
        let next_is_index = matches!(
            parents.get(position + 1).unwrap_or(leaf),
            ArgsPathSegment::Index(_)
        );
        container = match segment {
            ArgsPathSegment::Key(key) => {
                let object = container
                    .as_object_mut()
                    .ok_or_else(|| "Gemini partialArgs 路径与已组装参数类型冲突".to_string())?;
                if !object.contains_key(key) {
                    object.insert(key.clone(), empty_container(next_is_index));
                }
                object.get_mut(key).expect("inserted key")
            }
            ArgsPathSegment::Index(index) => {
                let array = container
                    .as_array_mut()
                    .ok_or_else(|| "Gemini partialArgs 路径与已组装参数类型冲突".to_string())?;
                if *index > array.len() {
                    return Err("Gemini partialArgs 数组下标不连续".to_string());
                }
                if *index == array.len() {
                    array.push(empty_container(next_is_index));
                }
                array.get_mut(*index).expect("pushed element")
            }
        };
    }

    match leaf {
        ArgsPathSegment::Key(key) => {
            let object = container
                .as_object_mut()
                .ok_or_else(|| "Gemini partialArgs 根路径必须是对象".to_string())?;
            if !object.contains_key(key) {
                object.insert(key.clone(), fresh_value(value));
                return Ok(LeafWrite::Created);
            }
            let existing = object.get_mut(key).expect("checked key");
            merge_leaf(existing, value)
        }
        ArgsPathSegment::Index(index) => {
            let array = container
                .as_array_mut()
                .ok_or_else(|| "Gemini partialArgs 数组路径与已组装参数类型冲突".to_string())?;
            if *index > array.len() {
                return Err("Gemini partialArgs 数组下标不连续".to_string());
            }
            if *index == array.len() {
                array.push(fresh_value(value));
                return Ok(LeafWrite::Created);
            }
            let existing = array.get_mut(*index).expect("checked index");
            merge_leaf(existing, value)
        }
    }
}

fn merge_leaf(existing: &mut Value, value: &PartialArgValue) -> Result<LeafWrite, String> {
    match (existing, value) {
        // 进入此分支前调用方已确认该路径的字符串仍在延续。
        (Value::String(existing), PartialArgValue::String(fragment)) => {
            existing.push_str(fragment);
            Ok(LeafWrite::Appended)
        }
        (existing, PartialArgValue::String(fragment)) => {
            if *existing == Value::String(fragment.clone()) {
                Ok(LeafWrite::Unchanged)
            } else {
                Err("Gemini partialArgs 同一路径存在冲突取值".to_string())
            }
        }
        (existing, PartialArgValue::Scalar(candidate)) => {
            if *existing == *candidate {
                Ok(LeafWrite::Unchanged)
            } else {
                Err("Gemini partialArgs 同一路径存在冲突取值".to_string())
            }
        }
    }
}

fn value_text(value: &PartialArgValue, open_string: bool) -> String {
    match value {
        // 延续中的字符串只输出开引号与内容，闭合引号由终结标记或最后一片补上。
        PartialArgValue::String(fragment) if open_string => {
            format!("\"{}", escape_fragment(fragment))
        }
        PartialArgValue::String(fragment) => {
            serde_json::to_string(fragment).unwrap_or_else(|_| "\"\"".to_string())
        }
        PartialArgValue::Scalar(value) => {
            serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
        }
    }
}

fn fresh_value(value: &PartialArgValue) -> Value {
    match value {
        PartialArgValue::String(fragment) => Value::String(fragment.clone()),
        PartialArgValue::Scalar(value) => value.clone(),
    }
}

fn empty_container(array: bool) -> Value {
    if array {
        Value::Array(Vec::new())
    } else {
        Value::Object(Map::new())
    }
}

fn parse_entry_value(
    object: &Map<String, Value>,
    raw_path: &str,
) -> Result<(Option<PartialArgValue>, Option<bool>), String> {
    let mut value: Option<PartialArgValue> = None;
    let mut will_continue = None;
    for (key, field) in object {
        match key.as_str() {
            "jsonPath" | "json_path" => {}
            "willContinue" | "will_continue" => {
                will_continue = Some(field.as_bool().ok_or_else(|| {
                    format!("Gemini partialArgs.willContinue 必须是布尔值（路径 {raw_path}）")
                })?);
            }
            "stringValue" | "string_value" => {
                let fragment = field.as_str().ok_or_else(|| {
                    format!("Gemini partialArgs.stringValue 必须是字符串（路径 {raw_path}）")
                })?;
                assign_value(
                    &mut value,
                    PartialArgValue::String(fragment.to_string()),
                    raw_path,
                )?;
            }
            "numberValue" | "number_value" => {
                let number = field.as_number().ok_or_else(|| {
                    format!("Gemini partialArgs.numberValue 必须是数字（路径 {raw_path}）")
                })?;
                assign_value(
                    &mut value,
                    PartialArgValue::Scalar(Value::Number(number.clone())),
                    raw_path,
                )?;
            }
            "boolValue" | "bool_value" => {
                let boolean = field.as_bool().ok_or_else(|| {
                    format!("Gemini partialArgs.boolValue 必须是布尔值（路径 {raw_path}）")
                })?;
                assign_value(
                    &mut value,
                    PartialArgValue::Scalar(Value::Bool(boolean)),
                    raw_path,
                )?;
            }
            "nullValue" | "null_value" => {
                if !field.is_null() {
                    return Err(format!(
                        "Gemini partialArgs.nullValue 必须是 null（路径 {raw_path}）"
                    ));
                }
                assign_value(&mut value, PartialArgValue::Scalar(Value::Null), raw_path)?;
            }
            other => {
                return Err(format!("Gemini partialArgs 含未知字段 {other}"));
            }
        }
    }
    Ok((value, will_continue))
}

fn assign_value(
    slot: &mut Option<PartialArgValue>,
    value: PartialArgValue,
    raw_path: &str,
) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!(
            "Gemini partialArgs 同时携带多个取值字段（路径 {raw_path}）"
        ));
    }
    *slot = Some(value);
    Ok(())
}

/// 解析 Vertex 使用的 jsonPath 子集：`$` 前缀、`.key` 与 `[index]`/`['key']`/`["key"]`。
fn parse_args_path(raw: &str) -> Result<ArgsPath, String> {
    let trimmed = raw.trim();
    let mut rest = trimmed
        .strip_prefix('$')
        .ok_or_else(|| format!("Gemini partialArgs.jsonPath 必须以 $ 开头: {raw}"))?;
    let mut segments = Vec::new();
    while !rest.is_empty() {
        if let Some(after_dot) = rest.strip_prefix('.') {
            let end = after_dot.find(['.', '[']).unwrap_or(after_dot.len());
            let key = &after_dot[..end];
            if key.is_empty() {
                return Err(format!("Gemini partialArgs.jsonPath 含空键: {raw}"));
            }
            segments.push(ArgsPathSegment::Key(key.to_string()));
            rest = &after_dot[end..];
        } else if rest.starts_with('[') {
            let (inner, after) = split_bracket(rest, raw)?;
            segments.push(parse_bracket_segment(inner, raw)?);
            rest = after;
        } else {
            return Err(format!("Gemini partialArgs.jsonPath 语法不合法: {raw}"));
        }
        if segments.len() > MAX_PARTIAL_ARG_PATH_SEGMENTS {
            return Err(format!("Gemini partialArgs.jsonPath 层级过深: {raw}"));
        }
    }
    if segments.is_empty() {
        return Err(format!("Gemini partialArgs.jsonPath 不能指向根: {raw}"));
    }
    Ok(segments)
}

fn split_bracket<'a>(rest: &'a str, raw: &str) -> Result<(&'a str, &'a str), String> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (offset, character) in rest.char_indices().skip(1) {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
                continue;
            }
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            ']' => return Ok((&rest[1..offset], &rest[offset + 1..])),
            _ => {}
        }
    }
    Err(format!("Gemini partialArgs.jsonPath 缺少 ]: {raw}"))
}

fn parse_bracket_segment(inner: &str, raw: &str) -> Result<ArgsPathSegment, String> {
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return Err(format!("Gemini partialArgs.jsonPath 含空下标: {raw}"));
    }
    if trimmed.starts_with('\'') || trimmed.starts_with('"') {
        return Ok(ArgsPathSegment::Key(parse_quoted_key(trimmed, raw)?));
    }
    if !trimmed.chars().all(|character| character.is_ascii_digit()) {
        return Err(format!("Gemini partialArgs.jsonPath 下标必须是数字: {raw}"));
    }
    let index = trimmed
        .parse::<usize>()
        .map_err(|_| format!("Gemini partialArgs.jsonPath 下标越界: {raw}"))?;
    if index > MAX_PARTIAL_ARG_ARRAY_INDEX {
        return Err(format!("Gemini partialArgs.jsonPath 下标越界: {raw}"));
    }
    Ok(ArgsPathSegment::Index(index))
}

fn parse_quoted_key(trimmed: &str, raw: &str) -> Result<String, String> {
    let quote = trimmed.chars().next().expect("non-empty");
    if trimmed.len() < 2 || !trimmed.ends_with(quote) {
        return Err(format!("Gemini partialArgs.jsonPath 引号未闭合: {raw}"));
    }
    if quote == '"' {
        return serde_json::from_str::<String>(trimmed)
            .map_err(|_| format!("Gemini partialArgs.jsonPath 字符串转义非法: {raw}"));
    }
    let body = &trimmed[1..trimmed.len() - 1];
    let mut key = String::with_capacity(body.len());
    let mut characters = body.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            key.push(character);
            continue;
        }
        let Some(escaped) = characters.next() else {
            return Err(format!("Gemini partialArgs.jsonPath 转义不完整: {raw}"));
        };
        key.push(match escaped {
            '\\' => '\\',
            '\'' => '\'',
            '"' => '"',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            other => {
                return Err(format!(
                    "Gemini partialArgs.jsonPath 含未知转义 \\{other}: {raw}"
                ))
            }
        });
    }
    Ok(key)
}

fn push_quoted(out: &mut String, value: &str) {
    if let Ok(quoted) = serde_json::to_string(value) {
        out.push_str(&quoted);
    } else {
        out.push_str("\"\"");
    }
}

/// 转义片段与整串转义等价：JSON 字符串转义逐字符进行，可直接拼接。
fn escape_fragment(fragment: &str) -> String {
    match serde_json::to_string(fragment) {
        Ok(quoted) if quoted.len() >= 2 => quoted[1..quoted.len() - 1].to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assemble(entries: &[Value]) -> (String, String) {
        let mut assembler = VertexPartialArgsAssembler::default();
        let mut delta = String::new();
        for entry in entries {
            delta.push_str(
                &assembler
                    .push_entries(std::slice::from_ref(entry))
                    .expect("partial arg applies"),
            );
        }
        let suffix = assembler.finish().expect("assembler finalizes");
        (delta, suffix)
    }

    fn parsed(delta: &str, suffix: &str) -> Value {
        serde_json::from_str::<Value>(&format!("{delta}{suffix}")).expect("valid assembled json")
    }

    #[test]
    fn assembles_chunked_string_with_leaf_marker() {
        let (delta, suffix) = assemble(&[
            json!({"jsonPath": "$.query", "stringValue": "line1", "willContinue": true}),
            json!({"jsonPath": "$.query", "stringValue": "line2", "willContinue": true}),
            json!({"jsonPath": "$.query"}),
        ]);
        assert_eq!(parsed(&delta, &suffix), json!({"query": "line1line2"}));
    }

    #[test]
    fn incrementally_emits_depth_first_values() {
        let mut assembler = VertexPartialArgsAssembler::default();
        let first = assembler
            .push_entries(&[json!({"jsonPath": "$.query", "stringValue": "rust"})])
            .expect("first leaf");
        assert_eq!(first, "{\"query\":\"rust\"");
        let second = assembler
            .push_entries(&[json!({"jsonPath": "$.limit", "numberValue": 5})])
            .expect("sibling leaf");
        assert_eq!(second, ",\"limit\":5");
        let suffix = assembler.finish().expect("finalize");
        assert_eq!(suffix, "}");
        assert_eq!(
            parsed(&format!("{first}{second}"), &suffix),
            json!({"query": "rust", "limit": 5})
        );
    }

    #[test]
    fn assembles_nested_objects_and_arrays() {
        let (delta, suffix) = assemble(&[
            json!({"jsonPath": "$.filter.status", "stringValue": "open"}),
            json!({"jsonPath": "$.filter.tags[0]", "stringValue": "a"}),
            json!({"jsonPath": "$.filter.tags[1]", "stringValue": "b"}),
            json!({"jsonPath": "$.filter.enabled", "boolValue": true}),
            json!({"jsonPath": "$.filter.missing", "nullValue": null}),
            json!({"jsonPath": "$['filter'][\"count\"]", "numberValue": 3}),
        ]);
        assert_eq!(
            parsed(&delta, &suffix),
            json!({
                "filter": {
                    "status": "open",
                    "tags": ["a", "b"],
                    "enabled": true,
                    "missing": null,
                    "count": 3,
                }
            })
        );
    }

    #[test]
    fn preserves_escaped_string_fragments() {
        let (delta, suffix) = assemble(&[
            json!({"jsonPath": "$.text", "stringValue": "quote:\" backslash:\\ ", "willContinue": true}),
            json!({"jsonPath": "$.text", "stringValue": "newline:\n tab:\t", "willContinue": true}),
            json!({"jsonPath": "$.text", "stringValue": "结束", "willContinue": false}),
        ]);
        assert_eq!(
            parsed(&delta, &suffix),
            json!({"text": "quote:\" backslash:\\ newline:\n tab:\t结束"})
        );
    }

    #[test]
    fn buffers_interleaved_string_leaves_until_final() {
        let mut assembler = VertexPartialArgsAssembler::default();
        let first = assembler
            .push_entries(&[json!({"jsonPath": "$.a", "stringValue": "one", "willContinue": true})])
            .expect("first string opens");
        assert_eq!(first, "{\"a\":\"one");
        // 另一条字符串插在中间：已输出尾部被占用，后续叶子全部缓冲。
        let second = assembler
            .push_entries(&[
                json!({"jsonPath": "$.b", "stringValue": "x", "willContinue": true}),
                json!({"jsonPath": "$.a", "stringValue": "-two", "willContinue": true}),
                json!({"jsonPath": "$.b", "stringValue": "y"}),
                json!({"jsonPath": "$.a"}),
            ])
            .expect("interleaved entries buffer");
        assert_eq!(second, "-two\"");
        let suffix = assembler.finish().expect("finalize");
        assert_eq!(
            parsed(&format!("{first}{second}"), &suffix),
            json!({"a": "one-two", "b": "xy"})
        );
    }

    #[test]
    fn buffers_non_depth_first_order_until_final() {
        let mut assembler = VertexPartialArgsAssembler::default();
        let first = assembler
            .push_entries(&[
                json!({"jsonPath": "$.outer.inner", "numberValue": 1}),
                json!({"jsonPath": "$.sibling", "numberValue": 2}),
                json!({"jsonPath": "$.outer.late", "numberValue": 3}),
            ])
            .expect("out of order entries buffer");
        // 只输出深度优先前沿，随后离开容器的新叶子进入缓冲。
        assert_eq!(first, "{\"outer\":{\"inner\":1");
        let suffix = assembler.finish().expect("finalize");
        assert_eq!(
            parsed(&first, &suffix),
            json!({"outer": {"inner": 1, "late": 3}, "sibling": 2})
        );
    }

    #[test]
    fn rejects_conflicting_values_and_types() {
        let mut assembler = VertexPartialArgsAssembler::default();
        assembler
            .push_entries(&[json!({"jsonPath": "$.a", "numberValue": 1})])
            .expect("first value");
        assert!(assembler
            .push_entries(&[json!({"jsonPath": "$.a", "numberValue": 2})])
            .is_err());
        assert!(assembler
            .push_entries(&[json!({"jsonPath": "$.a", "stringValue": "1"})])
            .is_err());
        assert!(assembler
            .push_entries(&[json!({"jsonPath": "$.a.b", "numberValue": 3})])
            .is_err());
    }

    #[test]
    fn rejects_malformed_paths_and_entries() {
        let mut assembler = VertexPartialArgsAssembler::default();
        for path in ["query", "$.", "$[", "$[x]", "$..a", "$"] {
            assert!(
                assembler
                    .push_entries(&[json!({"jsonPath": path, "stringValue": "v"})])
                    .is_err(),
                "path {path} must be rejected"
            );
        }
        assert!(assembler
            .push_entries(&[json!({"jsonPath": "$.a", "stringValue": "v", "boolValue": true})])
            .is_err());
        assert!(assembler
            .push_entries(&[json!({"jsonPath": "$.a", "bogusValue": "v"})])
            .is_err());
        assert!(assembler
            .push_entries(&[json!({"jsonPath": "$.a", "numberValue": "1"})])
            .is_err());
        assert!(assembler
            .push_entries(&[json!({"jsonPath": "$.a[2]", "stringValue": "v"})])
            .is_err());
    }

    #[test]
    fn parses_bracketed_keys_with_quotes_and_dots() {
        let (delta, suffix) = assemble(&[
            json!({"jsonPath": "$['weird]key']", "stringValue": "v"}),
            json!({"jsonPath": "$[\"a.b\"][0]", "numberValue": 1}),
            json!({"jsonPath": "$[\"a.b\"][1]", "numberValue": 2}),
            json!({"jsonPath": "$[\"a\\\"b\"]", "boolValue": true}),
        ]);
        assert_eq!(
            parsed(&delta, &suffix),
            json!({
                "weird]key": "v",
                "a.b": [1, 2],
                "a\"b": true,
            })
        );
    }

    #[test]
    fn preserves_emitted_prefix_when_buffering_nested_containers() {
        let mut assembler = VertexPartialArgsAssembler::default();
        let first = assembler
            .push_entries(&[json!({"jsonPath": "$.items[0].name", "stringValue": "a"})])
            .expect("nested leaf emits");
        assert_eq!(first, "{\"items\":[{\"name\":\"a\"");
        // 离开已打开容器的新叶子无法再插入文本，只能缓冲到终态；已输出前缀保持不变。
        let buffered = assembler
            .push_entries(&[
                json!({"jsonPath": "$.other", "numberValue": 1}),
                json!({"jsonPath": "$.items[1].name", "stringValue": "b"}),
            ])
            .expect("out of order leaves buffer");
        assert_eq!(buffered, "");
        let suffix = assembler.finish().expect("finalize");
        assert_eq!(
            parsed(&format!("{first}{buffered}"), &suffix),
            json!({"items": [{"name": "a"}, {"name": "b"}], "other": 1})
        );
    }

    #[test]
    fn terminal_incomplete_string_fails() {
        let mut assembler = VertexPartialArgsAssembler::default();
        assembler
            .push_entries(&[
                json!({"jsonPath": "$.a", "stringValue": "half", "willContinue": true}),
            ])
            .expect("open string");
        assert!(assembler.finish().is_err());
    }

    #[test]
    fn identical_duplicate_scalar_is_idempotent() {
        let mut assembler = VertexPartialArgsAssembler::default();
        let first = assembler
            .push_entries(&[json!({"jsonPath": "$.a", "numberValue": 1})])
            .expect("first value");
        let second = assembler
            .push_entries(&[json!({"jsonPath": "$.a", "numberValue": 1})])
            .expect("duplicate value");
        assert_eq!(first, "{\"a\":1");
        assert_eq!(second, "");
        assert_eq!(assembler.finish().expect("finalize"), "}");
    }

    #[test]
    fn empty_assembler_serializes_empty_object() {
        // 显式终结的零参数调用是合法调用：空组装结果必须是 `{}`。
        let mut assembler = VertexPartialArgsAssembler::default();
        assert_eq!(assembler.finish().expect("finalize"), "{}");
    }
}
