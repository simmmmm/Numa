use serde::Serialize;

pub const TEXT_LIMIT: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Block {
    pub list: bool,
    pub lines: Vec<Vec<Run>>,
}

fn item(line: &str) -> Option<&str> {
    match line {
        "-" => Some(""),
        _ => line.strip_prefix("- ").map(str::trim),
    }
}

pub fn parse(text: &str) -> Vec<Block> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut blocks: Vec<Block> = Vec::new();
    let mut open: Option<Block> = None;
    for line in text.split('\n').map(str::trim) {
        if line.is_empty() {
            blocks.extend(open.take());
            continue;
        }
        let (list, words) = match item(line) {
            Some(words) => (true, words),
            None => (false, line),
        };
        if open.as_ref().is_some_and(|block| block.list != list) {
            blocks.extend(open.take());
        }
        let block = open.get_or_insert_with(|| Block { list, lines: Vec::new() });
        if !words.is_empty() {
            block.lines.push(runs(words));
        }
    }
    blocks.extend(open);
    blocks.retain(|block| !block.lines.is_empty());
    blocks
}

enum Piece {
    Text(String),

    Mark { stars: usize, opens: bool, paired: bool },
}

fn runs(line: &str) -> Vec<Run> {
    let chars: Vec<char> = line.chars().collect();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut openers: Vec<usize> = Vec::new();
    let mut text = String::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '\\' && matches!(chars.get(at + 1), Some('*' | '\\' | '-')) {
            text.push(chars[at + 1]);
            at += 2;
            continue;
        }
        if c != '*' {
            text.push(c);
            at += 1;
            continue;
        }
        let width = chars[at..].iter().take_while(|&&star| star == '*').count();
        let before = at.checked_sub(1).map(|before| chars[before]);
        let after = chars.get(at + width).copied();
        at += width;
        if width > 3 {
            text.extend(std::iter::repeat_n('*', width));
            continue;
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(std::mem::take(&mut text)));
        }
        let (may_open, may_close) = (after.is_some_and(|c| !c.is_whitespace()), before.is_some_and(|c| !c.is_whitespace()));
        let mut left = width;
        while may_close && left > 0 {
            let Some(&top) = openers.last() else { break };
            let Piece::Mark { stars, .. } = pieces[top] else { break };
            if stars != left && left != 3 {
                break;
            }
            openers.pop();
            if let Piece::Mark { paired, .. } = &mut pieces[top] {
                *paired = true;
            }
            pieces.push(Piece::Mark { stars, opens: false, paired: true });
            left -= stars;
        }

        let widths: &[usize] = match left {
            3 => &[2, 1],
            2 => &[2],
            1 => &[1],
            _ => &[],
        };
        for &stars in widths {
            pieces.push(Piece::Mark { stars, opens: may_open, paired: false });
            if may_open {
                openers.push(pieces.len() - 1);
            }
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    let mut out: Vec<Run> = Vec::new();
    let (mut bold, mut italic) = (0usize, 0usize);
    for piece in pieces {
        let words = match piece {
            Piece::Text(words) => words,
            Piece::Mark { stars, opens, paired: true } => {
                let depth = if stars == 2 { &mut bold } else { &mut italic };
                *depth = if opens { *depth + 1 } else { depth.saturating_sub(1) };
                continue;
            }
            Piece::Mark { stars, .. } => "*".repeat(stars),
        };
        let (b, i) = (bold > 0, italic > 0);
        match out.last_mut() {
            Some(last) if last.bold == b && last.italic == i => last.text.push_str(&words),
            _ => out.push(Run { text: words, bold: b, italic: i }),
        }
    }
    out
}

fn escaped(words: &str) -> String {
    let mut out = String::with_capacity(words.len());
    let chars: Vec<char> = words.chars().collect();
    for (at, &c) in chars.iter().enumerate() {
        match c {
            '*' => out.push_str("\\*"),
            '\\' if matches!(chars.get(at + 1), None | Some('*' | '\\' | '-')) => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out
}

fn split_space(words: &str) -> (&str, &str, &str) {
    let core = words.trim();
    if core.is_empty() {
        return (words, "", "");
    }
    let start = words.len() - words.trim_start().len();
    (&words[..start], core, &words[start + core.len()..])
}

fn write_line(line: &[Run]) -> String {
    let close = |bold: bool, italic: bool| format!("{}{}", if italic { "*" } else { "" }, if bold { "**" } else { "" });
    let mut out = String::new();
    let (mut bold, mut italic) = (false, false);
    let mut gap = String::new();
    for run in line {
        let (lead, core, trail) = split_space(&run.text);
        gap.push_str(lead);
        if core.is_empty() {
            continue;
        }

        let reset = (bold && !run.bold) || (run.bold && !bold && italic);
        if reset {
            out.push_str(&close(bold, italic));
            (bold, italic) = (false, false);
        } else if italic && !run.italic {
            out.push('*');
            italic = false;
        }

        let reopen = !(reset && gap.is_empty());
        out.push_str(&std::mem::take(&mut gap));
        if reopen && run.bold && !bold {
            out.push_str("**");
            bold = true;
        }
        if reopen && run.italic && !italic {
            out.push('*');
            italic = true;
        }
        out.push_str(&escaped(core));
        gap.push_str(trail);
    }
    out.push_str(&close(bold, italic));
    out.trim().to_string()
}

pub fn write(blocks: &[Block]) -> String {
    let mut written: Vec<String> = Vec::new();
    for block in blocks {
        let lines: Vec<String> = block
            .lines
            .iter()
            .map(|line| write_line(line))
            .filter(|line| !line.is_empty())
            .map(|line| match block.list {
                true => format!("- {line}"),

                false if line == "-" || line.starts_with("- ") => format!("\\{line}"),
                false => line,
            })
            .collect();
        if !lines.is_empty() {
            written.push(lines.join("\n"));
        }
    }
    written.join("\n\n")
}

fn unlinked(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|at| open + at) else { break };
        let Some(end) = rest[close..].find(')').map(|at| close + at) else { break };
        let before = rest[..open].strip_suffix('!').unwrap_or(&rest[..open]);
        out.push_str(before);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn pasted(line: &str) -> String {
    let line: String = line.chars().map(|c| if c == '\t' { ' ' } else { c }).filter(|c| !c.is_control()).collect();
    let mut line = line.trim();
    loop {
        let quoted = line.strip_prefix('>').map(str::trim_start);
        let heading = line.trim_start_matches('#');
        let headed = (heading.len() < line.len() && line.len() - heading.len() <= 6 && heading.starts_with(' ')).then(|| heading.trim_start());
        match quoted.or(headed) {
            Some(inner) => line = inner,
            None => break,
        }
    }
    let marks: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
    if marks.len() >= 3 && marks.iter().all(|&c| c == marks[0]) && "-*_".contains(marks[0]) {
        return String::new();
    }
    let numbered = line.find(['.', ')']).filter(|&at| (1..=3).contains(&at) && line[..at].chars().all(|c| c.is_ascii_digit()) && line[at + 1..].starts_with(' '));
    let body = match numbered {
        Some(at) => Some(&line[at + 2..]),
        None => ["* ", "+ ", "• ", "◦ ", "‣ ", "– ", "— "].iter().find_map(|marker| line.strip_prefix(marker)),
    };
    let line = match body {
        Some(body) => format!("- {}", body.trim_start()),
        None => line.to_string(),
    };
    unlinked(&line)
}

pub fn clean(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<String> = text.split('\n').map(pasted).collect();
    write(&parse(&lines.join("\n")))
}

pub fn plain(text: &str) -> String {
    parse(text)
        .iter()
        .map(|block| block.lines.iter().map(|line| line.iter().map(|run| run.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, bold: bool, italic: bool) -> Run {
        Run { text: text.into(), bold, italic }
    }

    fn one_line(text: &str) -> Vec<Run> {
        let blocks = parse(text);
        assert_eq!(blocks.len(), 1, "{text:?}: {blocks:?}");
        blocks[0].lines[0].clone()
    }

    #[test]
    fn bold_italic_and_both_are_read_and_a_lone_star_stays_a_star() {
        assert_eq!(one_line("A **bold** and *quiet* day"), [run("A ", false, false), run("bold", true, false), run(" and ", false, false), run("quiet", false, true), run(" day", false, false)]);
        assert_eq!(one_line("***Both*** then"), [run("Both", true, true), run(" then", false, false)]);
        assert_eq!(one_line("**bold *and italic* inside**"), [run("bold ", true, false), run("and italic", true, true), run(" inside", true, false)]);
        assert_eq!(one_line("5 * 3 * 2 = 30"), [run("5 * 3 * 2 = 30", false, false)]);
        assert_eq!(one_line("5*3"), [run("5*3", false, false)]);
        assert_eq!(one_line("a lone ** here"), [run("a lone ** here", false, false)]);
        assert_eq!(one_line("**not closed"), [run("**not closed", false, false)]);
        assert_eq!(one_line("****"), [run("****", false, false)]);
        assert_eq!(one_line("\\*not italic\\*"), [run("*not italic*", false, false)]);
        assert_eq!(one_line("a**b**c"), [run("a", false, false), run("b", true, false), run("c", false, false)]);
    }

    #[test]
    fn paragraphs_lists_and_line_breaks() {
        let blocks = parse("First line\nsecond line\n\n\n- one\n-   two \n- \n\nAfter\n- right under");
        assert_eq!(blocks.len(), 4, "{blocks:?}");
        assert_eq!((blocks[0].list, blocks[0].lines.len()), (false, 2), "a line break is kept");
        assert_eq!((blocks[1].list, blocks[1].lines.clone()), (true, vec![vec![run("one", false, false)], vec![run("two", false, false)]]), "an empty item is left out");
        assert_eq!((blocks[2].list, blocks[3].list), (false, true), "a list may follow a paragraph's line");
        assert!(parse("  \n\n ").is_empty());
        assert_eq!(parse("-not an item")[0].list, false);
    }

    #[test]
    fn what_is_parsed_is_written_back_the_same() {
        for text in [
            "A **bold** and *quiet* day",
            "***Both*** then *a* **b**",
            "**bold *and italic* inside**",
            "5 \\* 3",
            "First\nsecond\n\n- one\n- **two**\n\nEnd.",
            "a path C:\\photos\\ here",
            "\\- not an item",
        ] {
            assert_eq!(write(&parse(text)), text, "{text:?}");
        }
    }

    fn styled(lines: &[Run]) -> Vec<(char, bool, bool)> {
        let all: Vec<(char, bool, bool)> = lines.iter().flat_map(|run| run.text.chars().map(move |c| (c, run.bold, run.italic))).collect();
        let trimmed: Vec<(char, bool, bool)> = all.iter().copied().skip_while(|(c, ..)| c.is_whitespace()).collect();
        trimmed.into_iter().map(|(c, bold, italic)| if c.is_whitespace() { (c, false, false) } else { (c, bold, italic) }).collect()
    }

    #[test]
    fn runs_a_client_made_are_written_so_they_read_back() {

        let line = vec![run("Day ", false, false), run(" one ", true, false), run("and", true, true), run(" two", false, true), run("*", false, false), run(" end\\", false, false)];
        let written = write(&[Block { list: false, lines: vec![line.clone()] }]);
        assert_eq!(styled(&parse(&written)[0].lines[0]), styled(&line), "{written:?}");
        let item = write(&[Block { list: false, lines: vec![vec![run("- looks like an item", false, false)]] }]);
        assert_eq!(parse(&item), [Block { list: false, lines: vec![vec![run("- looks like an item", false, false)]] }], "{item:?}");

        let inside = write(&[Block { list: false, lines: vec![vec![run("ab", true, true), run("cd", false, true)]] }]);
        assert!(!plain(&inside).contains('*'), "{inside:?}");
    }

    #[test]
    fn pasted_text_keeps_only_the_subset_and_cleaning_twice_is_cleaning_once() {
        let pasted = "# Day one\r\n\r\n> We left at **six**.\n\n* coffee\n+ tickets\n1. the train\n2) the boat\n• rain\n\n---\n\nSee [the map](https://example.com) and ![a picture](a.jpg).\n\n\n\nLast *line*\u{7}.";
        let cleaned = clean(pasted);
        assert_eq!(cleaned, "Day one\n\nWe left at **six**.\n\n- coffee\n- tickets\n- the train\n- the boat\n- rain\n\nSee the map and a picture.\n\nLast *line*.");
        for text in [pasted, "5 * 3", "**open", "a\\b", "\\\\*", "***x***y", "- - nested", "#hashtag stays", "  \t indented"] {
            let once = clean(text);
            assert_eq!(clean(&once), once, "{text:?}");
        }
        assert_eq!(clean("#hashtag stays"), "#hashtag stays");
        assert_eq!(plain("**Six** in the *morning*\n\n- coffee"), "Six in the morning\n\ncoffee");
    }
}
