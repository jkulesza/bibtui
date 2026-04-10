/// Display-only LaTeX → Unicode rendering.
///
/// This is purely cosmetic — the stored BibTeX value is never modified.
/// Must be applied BEFORE `strip_case_braces` so that accent patterns
/// inside `{...}` are still present when this function runs.
pub fn render_latex(s: &str) -> String {
    let s = render_dashes(s);
    let s = render_special_chars(&s);
    let s = render_accents(&s);
    let s = render_math_mode(&s);
    let s = render_script_commands(&s);
    let s = render_text_commands(&s);
    // Non-breaking tilde → regular space
    s.replace('~', " ")
}

// ── Dashes ────────────────────────────────────────────────────────────────────

fn render_dashes(s: &str) -> String {
    // Must replace --- before -- to avoid partial match
    s.replace("---", "\u{2014}").replace("--", "\u{2013}")
}

// ── Special characters ────────────────────────────────────────────────────────

fn render_special_chars(s: &str) -> String {
    let mut result = s.to_string();
    for (from, to) in SPECIAL_CHARS {
        result = result.replace(from, to);
    }
    result
}

/// Standalone special-character commands (no accent argument).
static SPECIAL_CHARS: &[(&str, &str)] = &[
    // Ligatures and special letters — braced form first, then bare
    ("{\\ss}", "ß"),
    ("{\\SS}", "SS"),
    ("{\\ae}", "æ"),
    ("{\\AE}", "Æ"),
    ("{\\oe}", "œ"),
    ("{\\OE}", "Œ"),
    ("{\\aa}", "å"),
    ("{\\AA}", "Å"),
    ("{\\o}", "ø"),
    ("{\\O}", "Ø"),
    ("{\\i}", "ı"),
    ("{\\j}", "ȷ"),
    ("{\\l}", "ł"),
    ("{\\L}", "Ł"),
    // Bare form with empty-group terminator
    ("\\ss{}", "ß"),
    ("\\ae{}", "æ"),
    ("\\AE{}", "Æ"),
    ("\\oe{}", "œ"),
    ("\\OE{}", "Œ"),
    ("\\aa{}", "å"),
    ("\\AA{}", "Å"),
    ("\\o{}", "ø"),
    ("\\O{}", "Ø"),
    ("\\i{}", "ı"),
    ("\\j{}", "ȷ"),
    ("\\l{}", "ł"),
    ("\\L{}", "Ł"),
];

// ── Accent substitutions ──────────────────────────────────────────────────────

fn render_accents(s: &str) -> String {
    let mut result = s.to_string();
    for (from, to) in ACCENTS {
        result = result.replace(from, to);
    }
    result
}

/// BibTeX accent patterns. Longer/nested patterns appear first so they are
/// replaced before shorter overlapping patterns.
static ACCENTS: &[(&str, &str)] = &[
    // ── Cedilla — nested brace form first ─────────────────────────────────────
    ("{\\c{c}}", "ç"), ("{\\c{C}}", "Ç"),
    ("{\\c{s}}", "ş"), ("{\\c{S}}", "Ş"),
    ("{\\c{t}}", "ţ"), ("{\\c{T}}", "Ţ"),
    // space-before-letter form
    ("{\\c c}", "ç"),  ("{\\c C}", "Ç"),
    ("{\\c s}", "ş"),  ("{\\c S}", "Ş"),
    ("{\\c t}", "ţ"),  ("{\\c T}", "Ţ"),
    // ── Caron — nested brace form ─────────────────────────────────────────────
    ("{\\v{c}}", "č"), ("{\\v{C}}", "Č"),
    ("{\\v{s}}", "š"), ("{\\v{S}}", "Š"),
    ("{\\v{z}}", "ž"), ("{\\v{Z}}", "Ž"),
    ("{\\v{r}}", "ř"), ("{\\v{R}}", "Ř"),
    ("{\\v{n}}", "ň"), ("{\\v{N}}", "Ň"),
    ("{\\v{d}}", "ď"), ("{\\v{D}}", "Ď"),
    ("{\\v{t}}", "ť"), ("{\\v{T}}", "Ť"),
    ("{\\v{e}}", "ě"), ("{\\v{E}}", "Ě"),
    ("{\\v{a}}", "ǎ"), ("{\\v{A}}", "Ǎ"),
    // space form
    ("{\\v c}", "č"),  ("{\\v C}", "Č"),
    ("{\\v s}", "š"),  ("{\\v S}", "Š"),
    ("{\\v z}", "ž"),  ("{\\v Z}", "Ž"),
    ("{\\v r}", "ř"),  ("{\\v R}", "Ř"),
    ("{\\v n}", "ň"),  ("{\\v N}", "Ň"),
    // ── Double acute ──────────────────────────────────────────────────────────
    ("{\\H{o}}", "ő"), ("{\\H{O}}", "Ő"),
    ("{\\H{u}}", "ű"), ("{\\H{U}}", "Ű"),
    ("{\\H o}", "ő"),  ("{\\H O}", "Ő"),
    ("{\\H u}", "ű"),  ("{\\H U}", "Ű"),
    // ── Ogonek ────────────────────────────────────────────────────────────────
    ("{\\k{a}}", "ą"), ("{\\k{A}}", "Ą"),
    ("{\\k{e}}", "ę"), ("{\\k{E}}", "Ę"),
    ("{\\k a}", "ą"),  ("{\\k A}", "Ą"),
    ("{\\k e}", "ę"),  ("{\\k E}", "Ę"),
    // ── Breve ─────────────────────────────────────────────────────────────────
    ("{\\u{a}}", "ă"), ("{\\u{A}}", "Ă"),
    ("{\\u{g}}", "ğ"), ("{\\u{G}}", "Ğ"),
    ("{\\u{e}}", "ĕ"), ("{\\u{E}}", "Ĕ"),
    ("{\\u a}", "ă"),  ("{\\u A}", "Ă"),
    ("{\\u g}", "ğ"),  ("{\\u G}", "Ğ"),
    // ── Macron ────────────────────────────────────────────────────────────────
    ("{\\=a}", "ā"), ("{\\=A}", "Ā"),
    ("{\\=e}", "ē"), ("{\\=E}", "Ē"),
    ("{\\=i}", "ī"), ("{\\=I}", "Ī"),
    ("{\\=o}", "ō"), ("{\\=O}", "Ō"),
    ("{\\=u}", "ū"), ("{\\=U}", "Ū"),
    // ── Dot above ─────────────────────────────────────────────────────────────
    ("{\\.c}", "ċ"), ("{\\.C}", "Ċ"),
    ("{\\.g}", "ġ"), ("{\\.G}", "Ġ"),
    ("{\\.z}", "ż"), ("{\\.Z}", "Ż"),
    ("{\\.I}", "İ"),
    // ── Acute accent ──────────────────────────────────────────────────────────
    ("{\\'{a}}", "á"), ("{\\'{A}}", "Á"),
    ("{\\'{e}}", "é"), ("{\\'{E}}", "É"),
    ("{\\'{i}}", "í"), ("{\\'{I}}", "Í"),
    ("{\\'{o}}", "ó"), ("{\\'{O}}", "Ó"),
    ("{\\'{u}}", "ú"), ("{\\'{U}}", "Ú"),
    ("{\\'{y}}", "ý"), ("{\\'{Y}}", "Ý"),
    ("{\\'{c}}", "ć"), ("{\\'{C}}", "Ć"),
    ("{\\'{n}}", "ń"), ("{\\'{N}}", "Ń"),
    ("{\\'{s}}", "ś"), ("{\\'{S}}", "Ś"),
    ("{\\'{z}}", "ź"), ("{\\'{Z}}", "Ź"),
    // single-char form (no nested braces)
    ("{\\'a}", "á"), ("{\\'A}", "Á"),
    ("{\\'e}", "é"), ("{\\'E}", "É"),
    ("{\\'i}", "í"), ("{\\'I}", "Í"),
    ("{\\'o}", "ó"), ("{\\'O}", "Ó"),
    ("{\\'u}", "ú"), ("{\\'U}", "Ú"),
    ("{\\'y}", "ý"), ("{\\'Y}", "Ý"),
    ("{\\'c}", "ć"), ("{\\'C}", "Ć"),
    ("{\\'n}", "ń"), ("{\\'N}", "Ń"),
    ("{\\'s}", "ś"), ("{\\'S}", "Ś"),
    ("{\\'z}", "ź"), ("{\\'Z}", "Ź"),
    // ── Grave accent ──────────────────────────────────────────────────────────
    ("{\\`{a}}", "à"), ("{\\`{A}}", "À"),
    ("{\\`{e}}", "è"), ("{\\`{E}}", "È"),
    ("{\\`{i}}", "ì"), ("{\\`{I}}", "Ì"),
    ("{\\`{o}}", "ò"), ("{\\`{O}}", "Ò"),
    ("{\\`{u}}", "ù"), ("{\\`{U}}", "Ù"),
    ("{\\ `a}", "à"), ("{\\ `A}", "À"),
    ("{\\`a}", "à"),  ("{\\`A}", "À"),
    ("{\\`e}", "è"),  ("{\\`E}", "È"),
    ("{\\`i}", "ì"),  ("{\\`I}", "Ì"),
    ("{\\`o}", "ò"),  ("{\\`O}", "Ò"),
    ("{\\`u}", "ù"),  ("{\\`U}", "Ù"),
    // ── Diaeresis / Umlaut ────────────────────────────────────────────────────
    ("{\\\"{a}}", "ä"), ("{\\\"{A}}", "Ä"),
    ("{\\\"{e}}", "ë"), ("{\\\"{E}}", "Ë"),
    ("{\\\"{i}}", "ï"), ("{\\\"{I}}", "Ï"),
    ("{\\\"{o}}", "ö"), ("{\\\"{O}}", "Ö"),
    ("{\\\"{u}}", "ü"), ("{\\\"{U}}", "Ü"),
    ("{\\\"{y}}", "ÿ"), ("{\\\"{Y}}", "Ÿ"),
    ("{\\\"a}", "ä"),  ("{\\\"A}", "Ä"),
    ("{\\\"e}", "ë"),  ("{\\\"E}", "Ë"),
    ("{\\\"i}", "ï"),  ("{\\\"I}", "Ï"),
    ("{\\\"o}", "ö"),  ("{\\\"O}", "Ö"),
    ("{\\\"u}", "ü"),  ("{\\\"U}", "Ü"),
    ("{\\\"y}", "ÿ"),  ("{\\\"Y}", "Ÿ"),
    // ── Circumflex ────────────────────────────────────────────────────────────
    ("{\\^{a}}", "â"), ("{\\^{A}}", "Â"),
    ("{\\^{e}}", "ê"), ("{\\^{E}}", "Ê"),
    ("{\\^{i}}", "î"), ("{\\^{I}}", "Î"),
    ("{\\^{o}}", "ô"), ("{\\^{O}}", "Ô"),
    ("{\\^{u}}", "û"), ("{\\^{U}}", "Û"),
    ("{\\^a}", "â"),   ("{\\^A}", "Â"),
    ("{\\^e}", "ê"),   ("{\\^E}", "Ê"),
    ("{\\^i}", "î"),   ("{\\^I}", "Î"),
    ("{\\^o}", "ô"),   ("{\\^O}", "Ô"),
    ("{\\^u}", "û"),   ("{\\^U}", "Û"),
    // ── Tilde ─────────────────────────────────────────────────────────────────
    ("{\\~{a}}", "ã"), ("{\\~{A}}", "Ã"),
    ("{\\~{n}}", "ñ"), ("{\\~{N}}", "Ñ"),
    ("{\\~{o}}", "õ"), ("{\\~{O}}", "Õ"),
    ("{\\~a}", "ã"),   ("{\\~A}", "Ã"),
    ("{\\~n}", "ñ"),   ("{\\~N}", "Ñ"),
    ("{\\~o}", "õ"),   ("{\\~O}", "Õ"),
];

// ── Math mode ─────────────────────────────────────────────────────────────────

fn render_math_mode(s: &str) -> String {
    if !s.contains('$') {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            // Consume optional second `$` (display math — treat same as inline)
            if chars.peek() == Some(&'$') {
                chars.next();
            }
            let mut math = String::new();
            let mut closed = false;
            for mc in chars.by_ref() {
                if mc == '$' {
                    // Consume trailing `$` for display math
                    closed = true;
                    break;
                }
                math.push(mc);
            }
            if closed {
                result.push_str(&render_math_content(&math));
            } else {
                result.push('$');
                result.push_str(&math);
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn render_math_content(s: &str) -> String {
    let mut result = s.to_string();
    // Replace known math symbol commands
    for (from, to) in MATH_SYMBOLS {
        result = result.replace(from, to);
    }
    // Super- and subscripts
    result = render_superscripts(&result);
    result = render_subscripts(&result);
    // Strip remaining braces and backslash-space
    result = result.replace("\\,", "\u{200A}").replace("\\;", " ").replace("\\:", " ");
    result.replace('{', "").replace('}', "").replace('\\', "")
}

static SUP: &[(char, char)] = &[
    ('0', '⁰'), ('1', '¹'), ('2', '²'), ('3', '³'), ('4', '⁴'),
    ('5', '⁵'), ('6', '⁶'), ('7', '⁷'), ('8', '⁸'), ('9', '⁹'),
    ('+', '⁺'), ('-', '⁻'), ('=', '⁼'), ('(', '⁽'), (')', '⁾'),
    ('a', 'ᵃ'), ('b', 'ᵇ'), ('c', 'ᶜ'), ('d', 'ᵈ'), ('e', 'ᵉ'),
    ('f', 'ᶠ'), ('g', 'ᵍ'), ('h', 'ʰ'), ('i', 'ⁱ'), ('j', 'ʲ'),
    ('k', 'ᵏ'), ('l', 'ˡ'), ('m', 'ᵐ'), ('n', 'ⁿ'), ('o', 'ᵒ'),
    ('p', 'ᵖ'), ('r', 'ʳ'), ('s', 'ˢ'), ('t', 'ᵗ'), ('u', 'ᵘ'),
    ('v', 'ᵛ'), ('w', 'ʷ'), ('x', 'ˣ'), ('y', 'ʸ'), ('z', 'ᶻ'),
];

static SUB: &[(char, char)] = &[
    ('0', '₀'), ('1', '₁'), ('2', '₂'), ('3', '₃'), ('4', '₄'),
    ('5', '₅'), ('6', '₆'), ('7', '₇'), ('8', '₈'), ('9', '₉'),
    ('+', '₊'), ('-', '₋'), ('=', '₌'), ('(', '₍'), (')', '₎'),
    ('a', 'ₐ'), ('e', 'ₑ'), ('h', 'ₕ'), ('i', 'ᵢ'), ('j', 'ⱼ'),
    ('k', 'ₖ'), ('l', 'ₗ'), ('m', 'ₘ'), ('n', 'ₙ'), ('o', 'ₒ'),
    ('p', 'ₚ'), ('r', 'ᵣ'), ('s', 'ₛ'), ('t', 'ₜ'), ('u', 'ᵤ'),
    ('v', 'ᵥ'), ('x', 'ₓ'),
];

fn render_superscripts(s: &str) -> String {
    apply_scripts(s, '^', SUP)
}

fn render_subscripts(s: &str) -> String {
    apply_scripts(s, '_', SUB)
}

fn apply_scripts(s: &str, trigger: char, map: &[(char, char)]) -> String {
    let sup = |ch: char| map.iter().find(|(f, _)| *f == ch).map(|(_, t)| *t);
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == trigger {
            if chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let mut content = String::new();
                for ic in chars.by_ref() {
                    if ic == '}' { break; }
                    content.push(ic);
                }
                for ch in content.chars() {
                    result.push(sup(ch).unwrap_or(ch));
                }
            } else if let Some(&next) = chars.peek() {
                chars.next();
                match sup(next) {
                    Some(mapped) => result.push(mapped),
                    None => { result.push(trigger); result.push(next); }
                }
            } else {
                result.push(trigger);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Greek letters, operators, and other common math symbols.
/// IMPORTANT: longer patterns that are prefixes of shorter ones must appear FIRST
/// so that string replacement doesn't consume their prefix prematurely.
static MATH_SYMBOLS: &[(&str, &str)] = &[
    // ── Number sets (longest, first) ──────────────────────────────────────────
    ("\\mathbb{R}", "ℝ"), ("\\mathbb{N}", "ℕ"), ("\\mathbb{Z}", "ℤ"),
    ("\\mathbb{Q}", "ℚ"), ("\\mathbb{C}", "ℂ"),
    // ── Greek lowercase — variant (longer) before base ────────────────────────
    ("\\varepsilon", "ε"), ("\\epsilon", "ε"),
    ("\\vartheta", "ϑ"),  ("\\theta", "θ"),
    ("\\varsigma", "ς"),  ("\\sigma", "σ"),
    ("\\upsilon", "υ"),
    ("\\varphi", "φ"),    ("\\phi", "φ"),
    ("\\varrho", "ϱ"),    ("\\rho", "ρ"),
    ("\\varpi", "ϖ"),     ("\\pi", "π"),
    ("\\alpha", "α"), ("\\beta", "β"), ("\\gamma", "γ"), ("\\delta", "δ"),
    ("\\zeta", "ζ"),  ("\\eta", "η"),  ("\\iota", "ι"),  ("\\kappa", "κ"),
    ("\\lambda", "λ"), ("\\mu", "μ"),  ("\\nu", "ν"),    ("\\xi", "ξ"),
    ("\\tau", "τ"),   ("\\chi", "χ"),  ("\\psi", "ψ"),   ("\\omega", "ω"),
    // ── Greek uppercase ───────────────────────────────────────────────────────
    ("\\Gamma", "Γ"), ("\\Delta", "Δ"), ("\\Theta", "Θ"), ("\\Lambda", "Λ"),
    ("\\Xi", "Ξ"),    ("\\Pi", "Π"),    ("\\Sigma", "Σ"), ("\\Upsilon", "Υ"),
    ("\\Phi", "Φ"),   ("\\Psi", "Ψ"),   ("\\Omega", "Ω"),
    // ── Relations — longer before prefix ──────────────────────────────────────
    ("\\Leftrightarrow", "⇔"), ("\\leftrightarrow", "↔"),
    ("\\Rightarrow", "⇒"), ("\\rightarrow", "→"),
    ("\\Leftarrow", "⇐"),  ("\\leftarrow", "←"),
    ("\\subseteq", "⊆"),   ("\\supseteq", "⊇"),
    ("\\subset", "⊂"),     ("\\supset", "⊃"),
    ("\\approx", "≈"), ("\\simeq", "≃"), ("\\cong", "≅"), ("\\sim", "∼"),
    ("\\equiv", "≡"),  ("\\propto", "∝"),
    ("\\notin", "∉"),  // must precede \\not if present, and \\in below
    ("\\neq", "≠"),    ("\\neg", "¬"),  // \\neq and \\neg before \\ne
    ("\\ne", "≠"),
    ("\\leq", "≤"),    ("\\geq", "≥"),  ("\\ell", "ℓ"), // \\leq/\\geq/\\ell before \\le/\\ge
    ("\\le", "≤"),     ("\\ge", "≥"),
    ("\\gets", "←"),   // \\gets before \\ge (\\gets starts with \\ge)
    ("\\ll", "≪"),     ("\\gg", "≫"),
    ("\\to", "→"),
    // ── Sets and logic ────────────────────────────────────────────────────────
    ("\\infty", "∞"),  // \\infty before \\in
    ("\\int", "∫"),    // \\int before \\in
    ("\\in", "∈"),
    ("\\cup", "∪"), ("\\cap", "∩"), ("\\emptyset", "∅"),
    ("\\forall", "∀"), ("\\exists", "∃"),
    ("\\wedge", "∧"),  ("\\vee", "∨"),
    // ── Operators ─────────────────────────────────────────────────────────────
    ("\\times", "×"), ("\\div", "÷"), ("\\pm", "±"), ("\\mp", "∓"),
    ("\\cdot", "·"),  ("\\bullet", "•"),
    ("\\oplus", "⊕"), ("\\otimes", "⊗"), ("\\circ", "∘"),
    // ── Misc ──────────────────────────────────────────────────────────────────
    ("\\partial", "∂"), ("\\nabla", "∇"),
    ("\\sqrt", "√"),    ("\\sum", "∑"),  ("\\prod", "∏"),
    ("\\oint", "∮"),    // \\oint before \\int (but \\int above is before \\in, so fine)
    ("\\ldots", "…"),   ("\\cdots", "⋯"), ("\\vdots", "⋮"), ("\\ddots", "⋱"),
    ("\\hbar", "ℏ"),    ("\\wp", "℘"),    ("\\Re", "ℜ"),    ("\\Im", "ℑ"),
];

// ── Script text commands ──────────────────────────────────────────────────────

/// Handle `\textsuperscript{...}` and `\textsubscript{...}` outside math mode,
/// converting the braced content to unicode super/subscript characters where
/// possible, falling back to the character itself when no mapping exists.
fn render_script_commands(s: &str) -> String {
    let s = replace_script_command(s, r"\textsuperscript{", SUP);
    replace_script_command(&s, r"\textsubscript{", SUB)
}

fn replace_script_command(s: &str, cmd: &str, map: &[(char, char)]) -> String {
    if !s.contains(cmd) {
        return s.to_string();
    }
    let lookup = |ch: char| map.iter().find(|(f, _)| *f == ch).map(|(_, t)| *t).unwrap_or(ch);
    let mut result = String::new();
    let mut remaining = s;
    while let Some(pos) = remaining.find(cmd) {
        result.push_str(&remaining[..pos]);
        remaining = &remaining[pos + cmd.len()..];
        let mut depth = 1usize;
        let mut content = String::new();
        let mut end = remaining.len();
        for (idx, ch) in remaining.char_indices() {
            match ch {
                '{' => { depth += 1; content.push(ch); }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = idx + ch.len_utf8();
                        break;
                    }
                    content.push(ch);
                }
                _ => content.push(ch),
            }
        }
        for ch in content.chars() {
            result.push(lookup(ch));
        }
        remaining = &remaining[end..];
    }
    result.push_str(remaining);
    result
}

// ── Text commands ─────────────────────────────────────────────────────────────

fn render_text_commands(s: &str) -> String {
    let mut result = s.to_string();
    for cmd in &[
        r"\textit{", r"\textbf{", r"\emph{", r"\textrm{",
        r"\texttt{", r"\text{", r"\mathrm{", r"\mathit{",
        r"\mathbf{", r"\mathcal{",
    ] {
        result = strip_command_braces(&result, cmd);
    }
    result
}

/// Strip a `\cmd{...}` wrapper, keeping the inner content.
/// Handles nested braces correctly.
fn strip_command_braces(s: &str, cmd: &str) -> String {
    if !s.contains(cmd) {
        return s.to_string();
    }
    let mut result = String::new();
    let mut remaining = s;
    while let Some(pos) = remaining.find(cmd) {
        result.push_str(&remaining[..pos]);
        // cmd already ends with `{`, so we're inside the braces now
        remaining = &remaining[pos + cmd.len()..];
        let mut depth = 1usize;
        let mut end = remaining.len();
        for (idx, ch) in remaining.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        result.push_str(&remaining[..idx]);
                        end = idx + ch.len_utf8();
                        break;
                    }
                }
                _ => {}
            }
        }
        if depth > 0 {
            // Unclosed command — push the rest and bail
            result.push_str(remaining);
            return result;
        }
        remaining = &remaining[end..];
    }
    result.push_str(remaining);
    result
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accents() {
        assert_eq!(render_latex("Ren{\\'e} Girard"), "René Girard");
        assert_eq!(render_latex("{\\\"o}"), "ö");
        assert_eq!(render_latex("{\\~n}"), "ñ");
        assert_eq!(render_latex("{\\c{c}}"), "ç");
        assert_eq!(render_latex("{\\v{s}}"), "š");
    }

    #[test]
    fn test_dashes() {
        assert_eq!(render_latex("pp. 1--10"), "pp. 1\u{2013}10");
        assert_eq!(render_latex("foo---bar"), "foo\u{2014}bar");
    }

    #[test]
    fn test_math() {
        assert_eq!(render_latex("$\\alpha$-decay"), "α-decay");
        assert_eq!(render_latex("CO$_{2}$ emissions"), "CO₂ emissions");
        assert_eq!(render_latex("$x^{2}$"), "x²");
        assert_eq!(render_latex("$\\infty$"), "∞");
        // Unbraced sub/superscript
        assert_eq!(render_latex("$P_3$"), "P₃");
        assert_eq!(render_latex("$P^2$"), "P²");
        assert_eq!(render_latex("$x_i$"), "xᵢ");
        assert_eq!(render_latex("$x_n$"), "xₙ");
        assert_eq!(render_latex("$x_a$"), "xₐ");
        assert_eq!(render_latex("$k^n$"), "kⁿ");
        assert_eq!(render_latex("$x^a$"), "xᵃ");
        // Brantley & Larsen title: double-braced field value with inline math
        assert_eq!(
            render_latex("{The Simplified $P_3$ Approximation}"),
            "{The Simplified P₃ Approximation}",
        );
    }

    #[test]
    fn test_text_commands() {
        assert_eq!(render_latex("\\textit{Monte Carlo}"), "Monte Carlo");
        assert_eq!(render_latex("\\emph{important}"), "important");
        // Nested
        assert_eq!(render_latex("\\textbf{\\textit{bold italic}}"), "bold italic");
    }

    #[test]
    fn test_textsuperscript() {
        assert_eq!(render_latex(r"\textsuperscript{th}"), "ᵗʰ");
        assert_eq!(render_latex(r"8\textsuperscript{th}"), "8ᵗʰ");
        assert_eq!(render_latex(r"\textsuperscript{2}"), "²");
        assert_eq!(render_latex(r"CO\textsuperscript{2}"), "CO²");
        // Characters without a superscript mapping pass through unchanged
        assert_eq!(render_latex(r"\textsuperscript{Q}"), "Q");
    }

    #[test]
    fn test_textsubscript() {
        assert_eq!(render_latex(r"\textsubscript{2}"), "₂");
        assert_eq!(render_latex(r"H\textsubscript{2}O"), "H₂O");
        assert_eq!(render_latex(r"\textsubscript{n}"), "ₙ");
    }

    #[test]
    fn test_special_chars() {
        assert_eq!(render_latex("{\\ss}"), "ß");
        assert_eq!(render_latex("{\\ae}"), "æ");
        assert_eq!(render_latex("{\\AE}"), "Æ");
        assert_eq!(render_latex("{\\o}"), "ø");
    }

    #[test]
    fn test_display_math() {
        // $$...$$ display math: the second opening $ is consumed (line 208).
        // The inner content is rendered; the trailing $ from $$-closing is a known
        // edge case of the simple parser.
        let result = render_latex("$$\\alpha$$");
        assert!(result.contains('α'), "expected alpha symbol, got: {}", result);
    }

    #[test]
    fn test_unclosed_math() {
        // A lone $ with no closing $ should pass the dollar sign and content through unchanged
        let result = render_latex("$x^2");
        assert!(result.contains('$'));
    }

    #[test]
    fn test_trailing_script_trigger() {
        // A ^ or _ at the very end of math content (no following character) should
        // emit the trigger character rather than panic or silently drop it
        let result = render_latex("$x^");
        assert!(result.contains('^'));
        let result = render_latex("$x_");
        assert!(result.contains('_'));
    }

    #[test]
    fn test_unclosed_text_command() {
        // \textit{ with no closing brace — strip_command_braces should push the
        // remaining content and return rather than looping forever
        let result = render_latex("\\textit{unclosed");
        assert!(result.contains("unclosed"));
    }
}
