//! Repair malformed markdown before parsing.
//!
//! This module handles common markdown issues that the parser doesn't handle
//! well, such as closing code fences on the same line as content.

use std::sync::LazyLock;

use streamdown_core::ParseState;

/// Common markdown code fence language tags (sorted longest-first for matching).
static COMMON_LANGUAGES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut langs = vec![
        "javascript", "typescript", "powershell", "dockerfile", "editorconfig",
        "clojure", "erlang", "elixir", "ocaml", "fsharp", "vbnet", "kotlin",
        "scala", "swift", "julia", "matlab", "perl", "lua", "haskell",
        "ruby", "php", "dart", "rust", "python", "java", "bash", "json",
        "yaml", "html", "css", "scss", "sass", "less", "markdown", "text",
        "plain", "diff", "patch", "ini", "cfg", "conf", "properties",
        "gitignore", "graphql", "regex", "vim", "emacs", "elisp", "lisp",
        "scheme", "racket", "prolog", "forth", "ada", "cobol", "fortran",
        "pascal", "delphi", "verilog", "vhdl", "asm", "nasm", "gas", "llvm",
        "wasm", "wat", "solidity", "vyper", "cairo", "move", "noir", "circom",
        "gdscript", "hlsl", "glsl", "wgsl", "metal", "cuda", "opencl",
        "gdscript", "makefile", "cmake", "dockerfile",
        "cpp", "cxx", "csharp", "golang", "py", "js", "ts", "cs", "rs",
        "kt", "pl", "ex", "ml", "fs", "vb", "sh", "zsh", "fish", "ps1",
        "cmd", "batch", "yml", "md", "txt", "toml", "xml", "sql", "go",
        "r", "c",
    ];
    // Sort longest first so "rust" matches before "r"
    langs.sort_by_key(|s| std::cmp::Reverse(s.len()));
    langs
});

/// Common code keywords that typically start a line of code.
static COMMON_CODE_KEYWORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "let", "pub", "fn", "use", "mod", "struct", "enum", "impl", "trait",
        "type", "const", "static", "mut", "ref", "match", "if", "else", "for",
        "while", "loop", "return", "break", "continue", "async", "await",
        "where", "move", "unsafe", "extern", "crate", "self", "Self", "super",
        "as", "in", "do", "try", "catch", "throw", "new", "delete", "class",
        "def", "var", "function", "import", "from", "yield", "lambda",
        "assert", "raise", "except", "finally", "with", "global", "nonlocal",
        "pass", "del", "print", "exec", "eval", "open", "dir", "vars",
        "locals", "globals", "hasattr", "getattr", "setattr", "delattr",
        "isinstance", "issubclass", "len", "range", "enumerate", "zip", "map",
        "filter", "reduce", "sum", "min", "max", "abs", "round", "divmod",
        "pow", "int", "long", "float", "complex", "str", "unicode", "list",
        "tuple", "dict", "set", "bytearray", "buffer", "memoryview", "bool",
        "chr", "unichr", "ord", "hex", "oct", "bin", "format", "repr",
        "ascii", "iter", "next", "slice", "reversed", "sorted", "all", "any",
        "callable", "classmethod", "staticmethod", "property", "super",
        "object", "import", "reload", "compile", "file", "include", "define",
        "ifdef", "ifndef", "endif", "pragma", "error", "warning", "line",
        "region", "endregion", "using", "namespace", "public", "private",
        "protected", "internal", "virtual", "override", "abstract", "sealed",
        "partial", "readonly", "volatile", "dynamic", "explicit", "implicit",
        "operator", "out", "params", "sizeof", "stackalloc", "switch", "this",
        "throw", "true", "false", "typeof", "checked", "unchecked", "fixed",
        "lock", "goto", "case", "default", "volatile", "register", "auto",
        "inline", "restrict", "typedef", "union", "signed", "unsigned",
        "short", "long", "char", "int", "float", "double", "void", "bool",
        "true", "false", "NULL", "nullptr", "const", "constexpr", "consteval",
        "constinit", "volatile", "mutable", "thread_local", "decltype",
        "typeof", "noexcept", "static_assert", "alignas", "alignof", "bitand",
        "bitor", "compl", "not", "not_eq", "or", "or_eq", "xor", "xor_eq",
        "and", "and_eq",
    ]
});

/// Repair a line of markdown, returning one or more normalized lines.
///
/// Handles:
/// - Embedded closing fences: `}```\n` becomes `}\n` + ```` ``` ```` (only when
///   in code block)
/// - Malformed opening fences: ````rustlet payload...` becomes ````rust` +
///   `let payload...` (only when outside code block)
pub fn repair_line(line: &str, state: &ParseState) -> Vec<String> {
    // Only check for embedded closing fence when we're inside a code block
    if state.is_in_code()
        && let Some(lines) = split_embedded_fence(line)
    {
        return lines;
    }

    // Check for malformed opening fence when we're outside a code block
    if !state.is_in_code()
        && let Some(lines) = split_malformed_opening_fence(line)
    {
        return lines;
    }

    vec![line.to_string()]
}

/// Split a line if it contains an embedded closing fence at the end.
/// e.g., `}``` ` becomes Some(vec![`}`, ```` ``` ````])
fn split_embedded_fence(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim_end();

    // Check for ``` at the end
    if let Some(stripped) = trimmed.strip_suffix("```")
        && !stripped.trim().is_empty()
    {
        return Some(vec![stripped.to_string(), "```".to_string()]);
    }

    // Check for ~~~ at the end
    if let Some(stripped) = trimmed.strip_suffix("~~~")
        && !stripped.trim().is_empty()
    {
        return Some(vec![stripped.to_string(), "~~~".to_string()]);
    }

    None
}

/// Split a malformed opening fence where the language tag got merged with the
/// first line of code. This happens in streaming markdown when the LLM emits
/// the fence and first code line without a separating newline.
///
/// e.g., ````rustlet payload = ...` becomes Some(vec![` ```rust `, ` let payload = ...`])
/// e.g., ````pythondef foo():` becomes Some(vec![` ```python `, ` def foo():`])
fn split_malformed_opening_fence(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let indent_str = &line[..indent];

    for prefix in ["```", "~~~"] {
        let Some(after_fence) = trimmed.strip_prefix(prefix) else {
            continue;
        };
        let after_trimmed = after_fence.trim_start();
        if after_trimmed.is_empty() {
            continue;
        }

        // Try each known language tag (longest first)
        for lang in COMMON_LANGUAGES.iter() {
            let Some(rest) = after_trimmed.strip_prefix(lang) else {
                continue;
            };
            let rest_trimmed = rest.trim_start();
            if rest_trimmed.is_empty() {
                continue;
            }

            // Check if the remaining content starts with a common code keyword
            for keyword in COMMON_CODE_KEYWORDS.iter() {
                if rest_trimmed.starts_with(keyword) {
                    return Some(vec![
                        format!("{}{}{}", indent_str, prefix, lang),
                        format!("{}{}", indent_str, rest_trimmed),
                    ]);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use streamdown_core::Code;

    use super::*;

    fn state_outside_code() -> ParseState {
        ParseState::new()
    }

    fn state_inside_code() -> ParseState {
        let mut state = ParseState::new();
        state.enter_code_block(Code::Backtick, Some("rust".to_string()));
        state
    }

    #[test]
    fn test_normal_line_unchanged() {
        assert_eq!(
            repair_line("hello world", &state_outside_code()),
            vec!["hello world"]
        );
        assert_eq!(
            repair_line("hello world", &state_inside_code()),
            vec!["hello world"]
        );
    }

    #[test]
    fn test_valid_fence_unchanged() {
        assert_eq!(repair_line("```", &state_outside_code()), vec!["```"]);
        assert_eq!(repair_line("   ```", &state_outside_code()), vec!["   ```"]);
        assert_eq!(
            repair_line("```rust", &state_outside_code()),
            vec!["```rust"]
        );
    }

    #[test]
    fn test_embedded_fence_not_split_outside_code_block() {
        // Outside code block, don't split
        assert_eq!(repair_line("}```", &state_outside_code()), vec!["}```"]);
        assert_eq!(
            repair_line("return x;```", &state_outside_code()),
            vec!["return x;```"]
        );
    }

    #[test]
    fn test_embedded_backtick_fence_split_in_code_block() {
        // Inside code block, split embedded fences
        assert_eq!(repair_line("}```", &state_inside_code()), vec!["}", "```"]);
        assert_eq!(
            repair_line("     }```", &state_inside_code()),
            vec!["     }", "```"]
        );
        assert_eq!(
            repair_line("return x;```", &state_inside_code()),
            vec!["return x;", "```"]
        );
    }

    #[test]
    fn test_embedded_tilde_fence_split_in_code_block() {
        assert_eq!(repair_line("}~~~", &state_inside_code()), vec!["}", "~~~"]);
        assert_eq!(
            repair_line("return x;~~~", &state_inside_code()),
            vec!["return x;", "~~~"]
        );
    }

    #[test]
    fn test_whitespace_only_before_fence_unchanged() {
        // Just whitespace before fence is a valid fence, don't split
        assert_eq!(repair_line("   ```", &state_inside_code()), vec!["   ```"]);
        assert_eq!(repair_line("\t```", &state_inside_code()), vec!["\t```"]);
    }

    // -- Malformed opening fence tests --

    #[test]
    fn test_malformed_rust_fence_split() {
        assert_eq!(
            repair_line("```rustlet payload = serde_json::json!({", &state_outside_code()),
            vec!["```rust", "let payload = serde_json::json!({"]
        );
    }

    #[test]
    fn test_malformed_rust_fence_split_with_indent() {
        assert_eq!(
            repair_line("  ```rustpub struct Foo {", &state_outside_code()),
            vec!["  ```rust", "  pub struct Foo {"]
        );
    }

    #[test]
    fn test_malformed_python_fence_split() {
        assert_eq!(
            repair_line("```pythondef foo():", &state_outside_code()),
            vec!["```python", "def foo():"]
        );
    }

    #[test]
    fn test_malformed_js_fence_split() {
        assert_eq!(
            repair_line("```jsconst x = 1;", &state_outside_code()),
            vec!["```js", "const x = 1;"]
        );
    }

    #[test]
    fn test_valid_fence_not_split() {
        // Valid fence lines should not be split
        assert_eq!(
            repair_line("```rust", &state_outside_code()),
            vec!["```rust"]
        );
        assert_eq!(
            repair_line("```python", &state_outside_code()),
            vec!["```python"]
        );
        assert_eq!(
            repair_line("```", &state_outside_code()),
            vec!["```"]
        );
    }

    #[test]
    fn test_non_keyword_not_split() {
        // If the rest doesn't start with a known keyword, don't split
        assert_eq!(
            repair_line("```rustic design", &state_outside_code()),
            vec!["```rustic design"]
        );
    }

    #[test]
    fn test_malformed_fence_not_split_inside_code_block() {
        // Inside a code block, malformed opening fences should NOT be split
        assert_eq!(
            repair_line("```rustlet x = 1;", &state_inside_code()),
            vec!["```rustlet x = 1;"]
        );
    }

    #[test]
    fn test_malformed_tilde_fence_split() {
        assert_eq!(
            repair_line("~~~pythondef bar():", &state_outside_code()),
            vec!["~~~python", "def bar():"]
        );
    }
}
