use crate::operation::SqlOperation;

pub(crate) fn classify(statement: &str) -> SqlOperation {
    let scanned = scan(statement);
    if scanned.has_multiple_statements {
        return SqlOperation::MultiStatement;
    }
    if scanned
        .tokens
        .windows(2)
        .any(|words| words == ["security", "definer"])
    {
        return SqlOperation::SecurityDefiner;
    }
    let words: Vec<&str> = scanned.tokens.iter().take(3).map(String::as_str).collect();
    match words.as_slice() {
        ["select", ..] => SqlOperation::Select,
        ["insert", ..] => SqlOperation::Insert,
        ["update", ..] => SqlOperation::Update,
        ["delete", ..] => SqlOperation::Delete,
        ["truncate", ..] => SqlOperation::Truncate,
        ["drop", "database", ..] => SqlOperation::DropDatabase,
        ["drop", "table", ..] => SqlOperation::DropTable,
        ["alter", "system", ..] => SqlOperation::AlterSystem,
        ["copy", ..] if scanned.tokens.iter().any(|word| word == "program") => {
            SqlOperation::CopyProgram
        }
        ["create", "extension", ..] => SqlOperation::CreateExtension,
        ["set", "global", ..] => SqlOperation::SetGlobal,
        ["load", "data", ..] => SqlOperation::LoadData,
        _ => SqlOperation::Other,
    }
}

pub(crate) fn has_token(statement: &str, token: &str) -> bool {
    scan(statement).tokens.iter().any(|word| word == token)
}

struct ScannedSql {
    tokens: Vec<String>,
    has_multiple_statements: bool,
}

fn scan(statement: &str) -> ScannedSql {
    let bytes = statement.as_bytes();
    let mut tokens = Vec::new();
    let mut has_code_in_current_statement = false;
    let mut saw_statement_separator = false;
    let mut has_multiple_statements = false;
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'\'' => skip_single_quoted(bytes, &mut index),
            b'"' => skip_double_quoted(bytes, &mut index),
            b'-' if bytes.get(index + 1) == Some(&b'-') => skip_line_comment(bytes, &mut index),
            b'/' if bytes.get(index + 1) == Some(&b'*') => skip_block_comment(bytes, &mut index),
            b'$' => {
                if !skip_dollar_quoted(bytes, &mut index) {
                    index += 1;
                }
            }
            b';' => {
                if has_code_in_current_statement {
                    saw_statement_separator = true;
                    has_code_in_current_statement = false;
                }
                index += 1;
            }
            byte if is_word_byte(byte) => {
                let start = index;
                while index < bytes.len() && is_word_byte(bytes[index]) {
                    index += 1;
                }
                if saw_statement_separator {
                    has_multiple_statements = true;
                }
                has_code_in_current_statement = true;
                tokens.push(statement[start..index].to_ascii_lowercase());
            }
            _ => index += 1,
        }
    }

    ScannedSql {
        tokens,
        has_multiple_statements,
    }
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_single_quoted(bytes: &[u8], index: &mut usize) {
    *index += 1;
    while *index < bytes.len() {
        if bytes[*index] == b'\'' {
            *index += 1;
            if bytes.get(*index) == Some(&b'\'') {
                *index += 1;
                continue;
            }
            break;
        }
        *index += 1;
    }
}

fn skip_double_quoted(bytes: &[u8], index: &mut usize) {
    *index += 1;
    while *index < bytes.len() {
        if bytes[*index] == b'"' {
            *index += 1;
            if bytes.get(*index) == Some(&b'"') {
                *index += 1;
                continue;
            }
            break;
        }
        *index += 1;
    }
}

fn skip_line_comment(bytes: &[u8], index: &mut usize) {
    *index += 2;
    while *index < bytes.len() && bytes[*index] != b'\n' {
        *index += 1;
    }
}

fn skip_block_comment(bytes: &[u8], index: &mut usize) {
    *index += 2;
    while *index + 1 < bytes.len() {
        if bytes[*index] == b'*' && bytes[*index + 1] == b'/' {
            *index += 2;
            return;
        }
        *index += 1;
    }
    *index = bytes.len();
}

fn skip_dollar_quoted(bytes: &[u8], index: &mut usize) -> bool {
    let start = *index;
    let mut end = start + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    if bytes.get(end) != Some(&b'$') {
        return false;
    }

    let tag = &bytes[start..=end];
    *index = end + 1;
    while *index + tag.len() <= bytes.len() {
        if &bytes[*index..*index + tag.len()] == tag {
            *index += tag.len();
            return true;
        }
        *index += 1;
    }
    *index = bytes.len();
    true
}
