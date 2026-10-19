use std::{iter::Copied, slice::Iter};

use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use parser::{OrGroup, ParsedQuery, Term, TermType};
use rayon::prelude::*;

const BYTES_1M: usize = 1024 * 1024;

/// Query parser for fuzzy matching with AND/OR combinators.
///
/// Grammar:
/// - `foo bar` -> foo AND bar (space = AND)
/// - `foo | bar` -> foo OR bar (` | ` = OR, must have spaces)
/// - OR binds tighter: `a b | c` -> a AND (b OR c)
///
/// Special operators (prefixes/suffixes):
/// - `^foo` -> prefix match (line starts with "foo")
/// - `foo$` -> suffix match (line ends with "foo")
/// - `'foo` -> exact substring match (no fuzzy)
/// - `!foo` -> inverse exact match (must NOT contain "foo")
/// - `!foo$` -> inverse suffix match
mod parser {
    use nom::{
        IResult, Parser,
        branch::alt,
        bytes::complete::{tag, take_while1},
        character::complete::space1,
        combinator::{map, rest},
        multi::separated_list1,
        sequence::preceded,
    };

    /// How to match a term against the line.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TermType {
        /// Default fuzzy matching
        Fuzzy,
        /// `^pattern` - line must start with pattern
        Prefix,
        /// `pattern$` - line must end with pattern
        Suffix,
        /// `'pattern` - exact substring match (not fuzzy)
        Exact,
        /// `!pattern` - line must NOT contain pattern
        InverseExact,
        /// `!pattern$` - line must NOT end with pattern
        InverseSuffix,
    }

    /// A single search term with its match type.
    pub struct Term {
        pub pattern: String,
        pub term_type: TermType,
    }

    /// Parses `^pattern` -> Prefix
    fn parse_prefix(input: &str) -> IResult<&str, Term> {
        map(preceded(tag("^"), rest), |pattern: &str| Term {
            pattern: pattern.to_string(),
            term_type: TermType::Prefix,
        })
        .parse(input)
    }

    /// Parses `'pattern` -> Exact
    fn parse_exact(input: &str) -> IResult<&str, Term> {
        map(preceded(tag("'"), rest), |pattern: &str| Term {
            pattern: pattern.to_string(),
            term_type: TermType::Exact,
        })
        .parse(input)
    }

    /// Parses `!pattern` -> `InverseExact`
    fn parse_inverse_exact(input: &str) -> IResult<&str, Term> {
        map(preceded(tag("!"), rest), |pattern: &str| Term {
            pattern: pattern.to_string(),
            term_type: TermType::InverseExact,
        })
        .parse(input)
    }

    /// Parses `pattern$` -> Suffix (pattern must be non-empty)
    fn parse_suffix(input: &str) -> IResult<&str, Term> {
        if !input.ends_with('$') || input.len() < 2 {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
        let pattern = &input[..input.len() - 1];
        Ok((
            "",
            Term {
                pattern: pattern.to_string(),
                term_type: TermType::Suffix,
            },
        ))
    }

    /// Parses `!pattern$` -> `InverseSuffix` (must not end with pattern)
    fn parse_inverse_suffix(input: &str) -> IResult<&str, Term> {
        let (rest, _) = tag::<_, _, nom::error::Error<&str>>("!")(input)?;
        if !rest.ends_with('$') || rest.len() < 2 {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
        let pattern = &rest[..rest.len() - 1];
        Ok((
            "",
            Term {
                pattern: pattern.to_string(),
                term_type: TermType::InverseSuffix,
            },
        ))
    }

    /// Parses plain `pattern` -> Fuzzy
    #[expect(clippy::unnecessary_wraps, reason = "more ergonomic `alt` matching")]
    fn parse_fuzzy(input: &str) -> IResult<&str, Term> {
        Ok((
            "",
            Term {
                pattern: input.to_string(),
                term_type: TermType::Fuzzy,
            },
        ))
    }

    /// Parse a raw token into a Term, extracting any operator prefix/suffix.
    fn parse_term_type(input: &str) -> Term {
        #[expect(
            clippy::expect_used,
            reason = "parse_fuzzy is catch all and will always suceed"
        )]
        alt((
            parse_inverse_suffix,
            parse_inverse_exact,
            parse_prefix,
            parse_exact,
            parse_suffix,
            parse_fuzzy,
        ))
        .parse(input)
        .map(|(_, term)| term)
        .expect("parse_fuzzy is a catch-all that always succeeds")
    }

    /// Terms connected by OR (` | `). Any term matching = group matches.
    pub struct OrGroup {
        pub terms: Vec<Term>,
    }

    /// Groups connected by AND (space). All groups must match.
    pub struct ParsedQuery {
        pub groups: Vec<OrGroup>,
    }

    /// Parses a single non-whitespace token into a Term.
    fn parse_term(input: &str) -> IResult<&str, Term> {
        map(take_while1(|c: char| !c.is_whitespace()), |s: &str| {
            parse_term_type(s)
        })
        .parse(input)
    }

    /// Parses terms separated by ` | ` into an OR group.
    fn parse_or_group(input: &str) -> IResult<&str, OrGroup> {
        map(separated_list1(tag(" | "), parse_term), |terms| OrGroup {
            terms,
        })
        .parse(input)
    }

    /// Parses OR groups separated by whitespace (AND semantics).
    fn parse_groups(input: &str) -> IResult<&str, Vec<OrGroup>> {
        separated_list1(space1, parse_or_group).parse(input)
    }

    impl ParsedQuery {
        pub fn parse(query: &str) -> Self {
            let input = query.trim();
            if input.is_empty() {
                return ParsedQuery { groups: vec![] };
            }
            match parse_groups(input) {
                Ok((_, groups)) => ParsedQuery { groups },
                Err(_) => ParsedQuery { groups: vec![] },
            }
        }
    }
}

/// A successful match of a term or a whole line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TermMatch {
    pub score: i64,
    /// sorted char indices into the line
    pub highlights: Vec<usize>,
}

pub struct FuzzyEngine {
    parsed_query: ParsedQuery,
    matcher: SkimMatcherV2,
}

impl FuzzyEngine {
    #[must_use]
    pub fn new(query: &str) -> Self {
        let matcher = SkimMatcherV2::default().element_limit(BYTES_1M);
        let matcher = matcher.smart_case();
        let parsed_query = ParsedQuery::parse(query);
        FuzzyEngine {
            parsed_query,
            matcher,
        }
    }

    /// Match a single term against the line.
    /// We match Skims behavior here and use the pattern length as a score for non-fuzzy matcher.
    fn match_term(&self, line: &str, term: &Term) -> Option<TermMatch> {
        match term.term_type {
            TermType::Fuzzy => self
                .matcher
                .fuzzy_indices(line, &term.pattern)
                .map(|(score, highlights)| TermMatch { score, highlights }),

            TermType::Exact => {
                let start = line.find(&term.pattern)?;
                Some(literal_match(line, start, &term.pattern))
            }

            TermType::Prefix => line
                .starts_with(&term.pattern)
                .then(|| literal_match(line, 0, &term.pattern)),

            TermType::Suffix => line
                .ends_with(&term.pattern)
                .then(|| literal_match(line, line.len() - term.pattern.len(), &term.pattern)),

            TermType::InverseExact => {
                if line.contains(&term.pattern) {
                    None
                } else {
                    Some(TermMatch::default())
                }
            }

            TermType::InverseSuffix => {
                if line.ends_with(&term.pattern) {
                    None
                } else {
                    Some(TermMatch::default())
                }
            }
        }
    }

    /// Match an OR group: returns the first matching term, or None if no term matches.
    fn match_or_group(&self, line: &str, group: &OrGroup) -> Option<TermMatch> {
        group
            .terms
            .iter()
            .find_map(|term| self.match_term(line, term))
    }

    /// list of entries to find `self.parsed_query` as (index, command) where we actually match on
    /// command
    pub fn match_all(&self, entries: &[(usize, &str)]) -> Vec<Match> {
        entries
            .par_iter()
            .filter_map(|&(original_idx, line)| {
                let TermMatch { score, highlights } = self.match_line(line)?;
                Some((original_idx, score, highlights))
            })
            .collect()
    }

    /// Matches all groups against `line`, an empty query matches every line.
    pub fn match_line(&self, line: &str) -> Option<TermMatch> {
        // AND: all groups must match
        let results: Vec<TermMatch> = self
            .parsed_query
            .groups
            .iter()
            .map(|group| self.match_or_group(line, group))
            .collect::<Option<_>>()?;

        let score = results.iter().map(|result| result.score).sum();
        let mut highlights: Vec<usize> = results
            .into_iter()
            .flat_map(|result| result.highlights)
            .collect();
        highlights.sort_unstable();
        highlights.dedup();

        Some(TermMatch { score, highlights })
    }
}

/// Match of a literal `pattern` found at byte offset `start` of `line`. Highlights are char
/// indices (like skim's `fuzzy_indices`) while the `str` search methods return byte offsets.
fn literal_match(line: &str, start: usize, pattern: &str) -> TermMatch {
    let first = line.get(..start).map_or(0, |before| before.chars().count());
    TermMatch {
        score: i64::try_from(pattern.len()).unwrap_or(i64::MAX),
        highlights: (first..first + pattern.chars().count()).collect(),
    }
}

/// index, score, highlight indices
// TODO: use a struct with named fields like `TermMatch`; also used by `FuzzyIndex` and the ui
pub type Match = (usize, i64, Vec<usize>);

impl From<Vec<Match>> for FuzzyIndex {
    fn from(results: Vec<Match>) -> Self {
        let mut indices = Vec::with_capacity(results.len());
        let mut scores = Vec::with_capacity(results.len());
        let mut highlight_indices = Vec::with_capacity(results.len());
        for (idx, score, highlights) in results {
            indices.push(idx);
            scores.push(score);
            highlight_indices.push(highlights);
        }
        Self {
            indices,
            scores,
            highlight_indices,
        }
    }
}

pub struct FuzzyIndex {
    /// indices into [`App.history`]
    indices: Vec<usize>,
    /// scores parallel to indices
    scores: Vec<i64>,
    /// highlight char indices, parallel to indices (match-rank indexed)
    highlight_indices: Vec<Vec<usize>>,
}

impl FuzzyIndex {
    #[must_use]
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// gets the first n indices
    pub fn first_n(&self, n: usize) -> Copied<Iter<'_, usize>> {
        let visible_count = n.min(self.indices.len());
        #[expect(
            clippy::indexing_slicing,
            reason = "using min ensures the slice is valid"
        )]
        self.indices[0..visible_count].iter().copied()
    }

    /// get the i-th index
    #[must_use]
    pub fn get(&self, index: usize) -> Option<usize> {
        self.indices.get(index).copied()
    }

    #[must_use]
    pub fn matcher_score(&self, index: usize) -> Option<i64> {
        self.scores.get(index).copied()
    }

    /// get the highlight indices
    #[must_use]
    pub fn highlight_indices(&self, index: usize) -> Option<&Vec<usize>> {
        self.highlight_indices.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_index_filtered_get() {
        let index = FuzzyIndex::from(vec![(5, 100, vec![0, 1]), (2, 50, vec![3])]);
        assert_eq!(index.get(0), Some(5));
        assert_eq!(index.get(1), Some(2));
        assert_eq!(index.get(2), None);
    }

    #[test]
    fn fuzzy_index_filtered_len() {
        let index = FuzzyIndex::from(vec![(5, 100, vec![]), (2, 50, vec![])]);
        assert_eq!(index.len(), 2);
        assert!(!index.is_empty());
    }

    #[test]
    fn fuzzy_index_filtered_first_n() {
        let index = FuzzyIndex::from(vec![(5, 100, vec![]), (2, 50, vec![]), (8, 10, vec![])]);
        let result: Vec<usize> = index.first_n(2).collect();
        assert_eq!(result, vec![5, 2]);
    }

    #[test]
    fn fuzzy_index_filtered_first_n_clamps_to_available() {
        let index = FuzzyIndex::from(vec![(1, 10, vec![])]);
        let result: Vec<usize> = index.first_n(10).collect();
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn fuzzy_index_filtered_scores_and_highlights() {
        let index = FuzzyIndex::from(vec![(5, 100, vec![0, 1]), (2, 50, vec![3, 4])]);
        assert_eq!(index.matcher_score(0), Some(100));
        assert_eq!(index.matcher_score(1), Some(50));
        assert_eq!(index.matcher_score(2), None);
        assert_eq!(index.highlight_indices(0), Some(&vec![0, 1]));
        assert_eq!(index.highlight_indices(1), Some(&vec![3, 4]));
        assert_eq!(index.highlight_indices(2), None);
    }

    fn matches(engine: &FuzzyEngine, line: &str) -> bool {
        engine.match_line(line).is_some()
    }

    #[test]
    fn single_term_matches() {
        let engine = FuzzyEngine::new("git");
        let TermMatch {
            score,
            highlights: indices,
        } = engine.match_line("git commit").unwrap();
        assert!(score > 0);
        assert_ne!(indices, [] as [usize; 0]);
    }

    #[test]
    fn single_term_no_match() {
        let engine = FuzzyEngine::new("xyz");
        assert!(engine.match_line("git commit").is_none());
    }

    #[test]
    fn and_both_terms_match() {
        let engine = FuzzyEngine::new("git commit");
        let TermMatch {
            score,
            highlights: indices,
        } = engine.match_line("git commit -m 'message'").unwrap();
        assert!(score > 0);
        assert_ne!(indices, [] as [usize; 0]);
    }

    #[test]
    fn and_one_term_fails() {
        let engine = FuzzyEngine::new("git xyz");
        assert!(engine.match_line("git commit").is_none());
    }

    #[test]
    fn empty_query_matches_everything() {
        let engine = FuzzyEngine::new("");
        assert_eq!(engine.match_line("git commit"), Some(TermMatch::default()));
    }

    #[test]
    fn whitespace_only_query_matches_everything() {
        let engine = FuzzyEngine::new("   ");
        assert_eq!(engine.match_line("git commit"), Some(TermMatch::default()));
    }

    #[test]
    fn highlight_indices_deduplicated() {
        let engine = FuzzyEngine::new("ab ba");
        let TermMatch {
            score,
            highlights: indices,
        } = engine.match_line("abba").unwrap();
        assert!(score > 0);
        let mut sorted_indices = indices.clone();
        sorted_indices.sort_unstable();
        sorted_indices.dedup();
        assert_eq!(
            indices, sorted_indices,
            "indices should be sorted and deduplicated"
        );
    }

    #[test]
    fn multiple_terms_score_accumulates() {
        let engine_single = FuzzyEngine::new("git");
        let engine_double = FuzzyEngine::new("git commit");

        let TermMatch {
            score: score_single,
            ..
        } = engine_single.match_line("git commit -m 'message'").unwrap();
        let TermMatch {
            score: score_double,
            ..
        } = engine_double.match_line("git commit -m 'message'").unwrap();

        assert!(
            score_double > score_single,
            "two matching terms should have higher score than one"
        );
    }

    #[test]
    fn or_first_alternative_matches() {
        let engine = FuzzyEngine::new("git | hg");
        let TermMatch {
            score,
            highlights: indices,
        } = engine.match_line("git commit").unwrap();
        assert!(score > 0);
        assert_ne!(indices, [] as [usize; 0]);
    }

    #[test]
    fn or_second_alternative_matches() {
        let engine = FuzzyEngine::new("git | hg");
        let TermMatch {
            score,
            highlights: indices,
        } = engine.match_line("hg commit").unwrap();
        assert!(score > 0);
        assert_ne!(indices, [] as [usize; 0]);
    }

    #[test]
    fn or_no_alternative_matches() {
        let engine = FuzzyEngine::new("git | hg");
        assert!(engine.match_line("svn commit").is_none());
    }

    #[test]
    fn or_with_and() {
        let engine = FuzzyEngine::new("git | hg commit");
        assert!(matches(&engine, "git commit"), "git commit should match");
        assert!(matches(&engine, "hg commit"), "hg commit should match");
        assert!(
            !matches(&engine, "git push"),
            "git push should not match (missing commit)"
        );
        assert!(
            !matches(&engine, "svn commit"),
            "svn commit should not match (missing git|hg)"
        );
    }

    #[test]
    fn or_binds_tighter_than_and() {
        let engine = FuzzyEngine::new("readme .md | .txt");
        assert!(matches(&engine, "readme.md"), "readme.md should match");
        assert!(matches(&engine, "readme.txt"), "readme.txt should match");
        assert!(!matches(&engine, "readme.rs"), "readme.rs should not match");
        assert!(
            !matches(&engine, "changelog.md"),
            "changelog.md should not match (missing readme)"
        );
        // This is the key precedence test: if AND bound tighter, "notes.txt" would match
        // because it would parse as (readme AND .md) OR .txt
        assert!(
            !matches(&engine, "notes.txt"),
            "notes.txt should not match (OR binds tighter than AND)"
        );
    }

    #[test]
    fn trailing_pipe_is_literal() {
        let engine = FuzzyEngine::new("git |");
        assert!(matches(&engine, "git | wc"), "should match git AND |");
        assert!(
            !matches(&engine, "git commit"),
            "should not match - missing |"
        );
    }

    #[test]
    fn leading_pipe_is_literal() {
        let engine = FuzzyEngine::new("| git");
        assert!(matches(&engine, "foo | git bar"), "should match | AND git");
        assert!(
            !matches(&engine, "git commit"),
            "should not match - missing |"
        );
    }

    #[test]
    fn lone_pipe_is_literal() {
        let engine = FuzzyEngine::new("|");
        assert!(matches(&engine, "foo | bar"), "should match literal |");
        assert!(!matches(&engine, "foo bar"), "should not match - no pipe");
    }

    #[test]
    fn pipe_without_spaces_is_literal() {
        let engine = FuzzyEngine::new("grep|wc");
        assert!(
            matches(&engine, "cat file | grep foo | wc -l"),
            "should match literal grep|wc"
        );
        assert!(
            !matches(&engine, "grep something"),
            "should not match grep alone (no pipe)"
        );
    }

    #[test]
    fn prefix_match() {
        let engine = FuzzyEngine::new("^git");

        let TermMatch {
            score,
            highlights: indices,
        } = engine.match_line("git commit").unwrap();
        assert!(score > 0, "should match - starts with git");
        assert_eq!(indices, vec![0, 1, 2]);

        assert!(
            !matches(&engine, "fugitive git"),
            "should not match - git not at start"
        );
    }

    #[test]
    fn suffix_match() {
        let engine = FuzzyEngine::new(".rs$");

        let TermMatch {
            score,
            highlights: indices,
        } = engine.match_line("main.rs").unwrap();
        assert!(score > 0, "should match - ends with .rs");
        assert_eq!(indices, vec![4, 5, 6]);

        assert!(
            !matches(&engine, "main.rs.bak"),
            "should not match - .rs not at end"
        );
    }

    #[test]
    fn exact_match() {
        let engine = FuzzyEngine::new("'git");
        assert!(
            matches(&engine, "git commit"),
            "should match - contains git"
        );
        assert!(
            !matches(&engine, "gti commit"),
            "should not match - gti is not git (no fuzzy)"
        );
    }

    #[test]
    fn empty_prefix_and_exact_match_everything() {
        for query in ["^", "'"] {
            let engine = FuzzyEngine::new(query);
            assert!(matches(&engine, "git commit"), "{query} should match");
        }
    }

    #[test]
    fn inverse_exact_match() {
        let engine = FuzzyEngine::new("!test");
        assert!(
            matches(&engine, "cargo build"),
            "should match - does not contain test"
        );
        assert!(
            !matches(&engine, "cargo test"),
            "should not match - contains test"
        );
    }

    #[test]
    fn inverse_suffix_match() {
        let engine = FuzzyEngine::new("!.tmp$");
        assert!(
            matches(&engine, "main.rs"),
            "should match - does not end with .tmp"
        );
        assert!(
            !matches(&engine, "file.tmp"),
            "should not match - ends with .tmp"
        );
    }

    #[test]
    fn combined_operators() {
        let engine = FuzzyEngine::new("^git !test");
        assert!(
            matches(&engine, "git commit"),
            "should match - starts with git, no test"
        );
        assert!(
            !matches(&engine, "git test"),
            "should not match - contains test"
        );
        assert!(
            !matches(&engine, "fugitive git"),
            "should not match - doesn't start with git"
        );
    }

    #[test]
    fn operators_with_or() {
        let engine = FuzzyEngine::new(".rs$ | .py$");
        assert!(matches(&engine, "main.rs"), "should match .rs");
        assert!(matches(&engine, "main.py"), "should match .py");
        assert!(!matches(&engine, "main.go"), "should not match .go");
    }

    fn highlighted(line: &str, indices: &[usize]) -> Option<String> {
        indices
            .iter()
            .map(|&index| line.chars().nth(index))
            .collect()
    }

    proptest::proptest! {
        #[test]
        fn highlights_are_sorted_unique_char_indices(line in ".*", query in ".*") {
            let indices = FuzzyEngine::new(&query).match_line(&line).unwrap_or_default().highlights;
            let char_count = line.chars().count();
            proptest::prop_assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
            proptest::prop_assert!(indices.iter().all(|&index| index < char_count));
        }

        #[test]
        fn exact_highlights_the_pattern(
            before in ".*",
            pattern in "[^\\s]+",
            after in ".*",
        ) {
            let line = format!("{before}{pattern}{after}");
            let indices = FuzzyEngine::new(&format!("'{pattern}")).match_line(&line).unwrap_or_default().highlights;
            proptest::prop_assert_eq!(highlighted(&line, &indices), Some(pattern));
        }

        #[test]
        fn suffix_highlights_the_pattern(before in ".*", pattern in "[^\\s$'^!][^\\s$]*") {
            let line = format!("{before}{pattern}");
            let indices = FuzzyEngine::new(&format!("{pattern}$")).match_line(&line).unwrap_or_default().highlights;
            proptest::prop_assert_eq!(highlighted(&line, &indices), Some(pattern));
        }

        #[test]
        fn line_matching_its_own_prefix_is_kept(prefix in "[^\\s]*", rest in ".*") {
            let line = format!("{prefix}{rest}");
            let engine = FuzzyEngine::new(&format!("^{prefix}"));
            proptest::prop_assert_eq!(engine.match_all(&[(0, line.as_str())]).len(), 1);
        }
    }
}
