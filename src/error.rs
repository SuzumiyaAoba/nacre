use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    message: String,
    source_name: Option<String>,
    source_line: Option<String>,
}

impl CompileError {
    pub(crate) fn new(line: usize, message: String) -> Self {
        Self::with_span(line, 1, line, 1, message)
    }

    pub(crate) fn with_span(
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
        message: String,
    ) -> Self {
        Self {
            line,
            column: column.max(1),
            end_line: end_line.max(line),
            end_column: end_column.max(column.max(1)),
            message,
            source_name: None,
            source_line: None,
        }
    }

    pub(crate) fn with_source_context(
        mut self,
        source_name: impl Into<String>,
        source: &str,
    ) -> Self {
        if self.line == 0 || self.source_line.is_some() {
            return self;
        }
        self.source_name = Some(source_name.into());
        self.source_line = source
            .lines()
            .nth(self.line.saturating_sub(1))
            .map(str::to_string);
        self
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn column(&self) -> usize {
        self.column
    }

    pub fn end_line(&self) -> usize {
        self.end_line
    }

    pub fn end_column(&self) -> usize {
        self.end_column
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source_name.as_deref()
    }

    pub fn source_line(&self) -> Option<&str> {
        self.source_line.as_deref()
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{},\"message\":\"{}\",\"source_name\":{},\"source_line\":{}}}",
            self.line,
            self.column,
            self.end_line,
            self.end_column,
            json_escape(&self.message),
            json_option(self.source_name.as_deref()),
            json_option(self.source_line.as_deref())
        )
    }
}

fn json_option(value: Option<&str>) -> String {
    value
        .map(|value| format!("\"{}\"", json_escape(value)))
        .unwrap_or_else(|| "null".to_string())
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            return write!(f, "{}", self.message);
        }
        write!(f, "line {}:{}: {}", self.line, self.column, self.message)?;
        if let Some(source_line) = &self.source_line {
            let label = self.source_name.as_deref().unwrap_or("<source>");
            let width = self.line.to_string().len();
            let column = self.column.max(1);
            let marker_len = if self.end_line == self.line {
                self.end_column.saturating_sub(column).max(1)
            } else {
                1
            };
            write!(
                f,
                "\n --> {label}:{}:{}\n{:>width$} | {}\n{:>width$} | {}{}",
                self.line,
                column,
                self.line,
                source_line,
                "",
                " ".repeat(column.saturating_sub(1)),
                "^".repeat(marker_len),
                width = width
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for CompileError {}
