use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Syntax highlighter using syntect
pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Highlight a code block with the given language
    pub fn highlight_code(&self, code: &str, lang: &str) -> Vec<Line<'static>> {
        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .or_else(|| self.syntax_set.find_syntax_by_extension(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let mut result = Vec::new();

        for line in LinesWithEndings::from(code) {
            match highlighter.highlight_line(line, &self.syntax_set) {
                Ok(ranges) => {
                    let spans: Vec<Span<'static>> = ranges
                        .iter()
                        .map(|(style, text)| {
                            Span::styled(text.to_string(), syntect_to_ratatui_style(style))
                        })
                        .collect();
                    result.push(Line::from(spans));
                }
                Err(_) => {
                    // Fall back to plain text
                    result.push(Line::from(line.to_string()));
                }
            }
        }

        result
    }

    /// Check if a language is supported
    pub fn supports_language(&self, lang: &str) -> bool {
        self.syntax_set.find_syntax_by_token(lang).is_some()
            || self.syntax_set.find_syntax_by_extension(lang).is_some()
    }
}

/// Convert syntect style to ratatui style
fn syntect_to_ratatui_style(style: &SyntectStyle) -> Style {
    let fg = Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    );

    let mut ratatui_style = Style::default().fg(fg);

    if style.font_style.contains(syntect::highlighting::FontStyle::BOLD) {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(syntect::highlighting::FontStyle::ITALIC) {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(syntect::highlighting::FontStyle::UNDERLINE) {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }

    ratatui_style
}

/// Parse preview lines and identify code blocks
/// Returns a list of (line_idx, is_code, language) tuples
pub fn parse_code_blocks(lines: &[String]) -> Vec<CodeBlockInfo> {
    let mut result = Vec::new();
    let mut in_code_block = false;
    let mut current_lang = String::new();
    let mut block_start = 0;

    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with("```") {
            if in_code_block {
                // End of code block
                result.push(CodeBlockInfo {
                    start: block_start,
                    end: idx,
                    language: current_lang.clone(),
                });
                in_code_block = false;
                current_lang.clear();
            } else {
                // Start of code block
                in_code_block = true;
                block_start = idx + 1; // Skip the ``` line
                current_lang = line.trim_start_matches('`').trim().to_string();
            }
        }
    }

    result
}

#[derive(Debug, Clone)]
pub struct CodeBlockInfo {
    pub start: usize,
    pub end: usize,
    pub language: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_code_blocks() {
        let lines: Vec<String> = vec![
            "Some text".to_string(),
            "```rust".to_string(),
            "fn main() {}".to_string(),
            "```".to_string(),
            "More text".to_string(),
        ];

        let blocks = parse_code_blocks(&lines);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start, 2);
        assert_eq!(blocks[0].end, 3);
        assert_eq!(blocks[0].language, "rust");
    }

    #[test]
    fn test_highlighter_supports_rust() {
        let highlighter = Highlighter::new();
        assert!(highlighter.supports_language("rust"));
        assert!(highlighter.supports_language("python"));
        assert!(highlighter.supports_language("rs"));
        assert!(highlighter.supports_language("py"));
    }
}
