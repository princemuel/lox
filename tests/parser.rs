#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};

use lox::prelude::{Location, Parser, Scanner, SyntaxError};

fn expr(src: &str) -> String {
    match Parser::new(src).parse_expression() {
        Ok(tree) => tree.to_string(),
        Err(errs) => panic!("{src:?} should parse as an expression: {errs:?}"),
    }
}

fn program(src: &str) -> Vec<String> {
    match Parser::new(src).parse_program() {
        Ok(stmts) => stmts.iter().map(ToString::to_string).collect(),
        Err(errs) => panic!("{src:?} should parse: {errs:?}"),
    }
}

/// Every error line the parser reports for `src`, exactly as printed.
fn errors(src: &str) -> Vec<String> {
    match Parser::new(src).parse_program() {
        Ok(stmts) => panic!("{src:?} should fail to parse, got {} statement(s)", stmts.len()),
        Err(errs) => errs.iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn precedence_and_associativity() {
    let cases = [
        ("a or b and c", "(or a (and b c))"),
        ("a and b or c", "(or (and a b) c)"),
        ("true == 1 < 2", "(== true (< 1.0 2.0))"),
        ("1 < 2 == true", "(== (< 1.0 2.0) true)"),
        ("1 - 2 - 3", "(- (- 1.0 2.0) 3.0)"),
        ("1 - 2 * 3 - 4", "(- (- 1.0 (* 2.0 3.0)) 4.0)"),
        ("-a * b", "(* (- a) b)"),
        ("-a.b", "(- (. a b))"),
        ("!!a", "(! (! a))"),
        ("a.b.c", "(. (. a b) c)"),
        ("f(1)(2).x", "(. ((f 1.0) 2.0) x)"),
        ("a = b = c", "(= a (= b c))"),
        ("a.b = c or d", "(= (. a b) (or c d))"),
        ("(5 - (3 - 1)) + -1", "(+ (group (- 5.0 (group (- 3.0 1.0)))) (- 1.0))"),
    ];
    for (src, want) in cases {
        assert_eq!(expr(src), want, "source: {src}");
    }
}

#[test]
fn statements() {
    let cases: [(&str, &[&str]); 8] = [
        ("print 1; print 2;", &["(print 1.0)", "(print 2.0)"]),
        ("var a; var b = 1;", &["(var a)", "(var b 1.0)"]),
        ("a = 1;", &["(expr (= a 1.0))"]),
        ("if (a) print 1; else if (b) print 2; else print 3;", &[
            "(if a (print 1.0) (if b (print 2.0) (print 3.0)))"
        ]),
        // dangling else binds to the nearest `if`
        ("if (a) if (b) print 1; else print 2;", &["(if a (if b (print 1.0) (print 2.0)))"]),
        ("fun f(a, b) { return; }", &["(def f (a b) (return))"]),
        ("class A < B { m() { return 1; } }", &["(class A < B (def m () (return 1.0)))"]),
        ("", &[]),
    ];

    for (src, want) in cases {
        assert_eq!(program(src), want, "source: {src}");
    }
}

#[test]
fn for_is_desugared_to_while() {
    assert_eq!(program("for (var i = 0; i < 3; i = i + 1) print i;"), ["(block (var i 0.0) \
                                                                        (while (< i 3.0) (block \
                                                                        (print i) (expr (= i (+ \
                                                                        i 1.0))))))"]);
    assert_eq!(program("for (;;) print 1;"), ["(while true (print 1.0))"]);
}

#[test]
fn tokens_know_their_line() {
    // a string reports the line it *ends* on, like jlox
    let src = "a\n\"x\ny\"\nb // comment\nd";
    let lines: Vec<usize> = Scanner::new(src)
        .filter_map(Result::ok)
        .map(|t| t.line)
        .collect();
    assert_eq!(lines, [1, 3, 4, 5]);
}

#[test]
fn error_display_has_the_books_three_shapes() {
    let at = |location| SyntaxError { line: 3, location, message: "msg.".to_owned() };
    assert_eq!(at(Location::Unspecified).to_string(), "[line 3] Error: msg.");
    assert_eq!(at(Location::Token("b".to_owned())).to_string(), "[line 3] Error at 'b': msg.");
    assert_eq!(at(Location::End).to_string(), "[line 3] Error at end: msg.");
}

#[test]
fn single_syntax_errors() {
    let cases = [
        ("a + b = c;", "[line 1] Error at '=': Invalid assignment target."),
        ("(a) = 1;", "[line 1] Error at '=': Invalid assignment target."),
        ("class F { m() { this = 1; } }", "[line 1] Error at '=': Invalid assignment target."),
        ("a b;", "[line 1] Error at 'b': Expect ';' after expression."),
        ("else print 1;", "[line 1] Error at 'else': Expect expression."),
        ("for (;;) var a = 1;", "[line 1] Error at 'var': Expect expression."),
        ("var 1 = 2;", "[line 1] Error at '1': Expect variable name."),
        ("super;", "[line 1] Error at ';': Expect '.' after 'super'."),
        ("a.1;", "[line 1] Error at '1': Expect property name after '.'."),
        ("print \"a\" \"b\";", "[line 1] Error at '\"b\"': Expect ';' after value."),
        // the error is on the token that *follows* the missing `;`
        ("print 1\nprint 2;", "[line 2] Error at 'print': Expect ';' after value."),
        // end of input: "at end", on the last line (a trailing newline counts)
        ("print 1", "[line 1] Error at end: Expect ';' after value."),
        ("print 1\n\n", "[line 3] Error at end: Expect ';' after value."),
        ("{ print 1;\n", "[line 2] Error at end: Expect '}' after block."),
        // lexical errors
        ("\"abc", "[line 1] Error: Unterminated string."),
        ("print 1;\n\"abc\ndef\n", "[line 4] Error: Unterminated string."),
    ];
    for (src, want) in cases {
        assert_eq!(errors(src), [want], "source: {src:?}");
    }
}

#[test]
fn several_errors_in_one_run() {
    // `synchronize` skips to the next statement, so both mistakes are found
    // and the second one isn't drowned in follow-on noise.
    assert_eq!(errors("var a = ;\nprint 1 +;\nprint 3;"), [
        "[line 1] Error at ';': Expect expression.",
        "[line 2] Error at ';': Expect expression.",
    ]);
    // ...including inside a block, where recovery is per statement.
    assert_eq!(errors("{\n  var = 1;\n  print ;\n}\nprint 1;"), [
        "[line 2] Error at '=': Expect variable name.",
        "[line 3] Error at ';': Expect expression.",
    ]);
    // a failure in a class body drops that class but not what follows it
    assert_eq!(errors("class A {\n  1() {}\n}\nprint ;"), [
        "[line 2] Error at '1': Expect method name.",
        "[line 4] Error at ';': Expect expression.",
    ]);
}

#[test]
fn lexical_and_syntax_errors_are_reported_together() {
    // the bad character is skipped, exactly as if it weren't there
    assert_eq!(errors("foo(a | b);"), [
        "[line 1] Error: Unexpected character.",
        "[line 1] Error at 'b': Expect ')' after arguments.",
    ]);
    assert_eq!(errors("print \u{20ac};"), [
        "[line 1] Error: Unexpected character.",
        "[line 1] Error at ';': Expect expression."
    ]);
}

#[test]
fn invalid_assignment_target_is_reported_but_not_thrown() {
    // jlox records it and keeps parsing the same statement, so the *next*
    // statement still gets its own (separate) report.
    assert_eq!(errors("a + b = c;\nvar = 1;"), [
        "[line 1] Error at '=': Invalid assignment target.",
        "[line 2] Error at '=': Expect variable name.",
    ]);
    // Decisive case: because the statement carries on after the report, the
    // stray `d` is noticed too. If it were thrown, `synchronize` would skip it.
    assert_eq!(errors("a + b = c d;"), [
        "[line 1] Error at '=': Invalid assignment target.",
        "[line 1] Error at 'd': Expect ';' after expression.",
    ]);
}

#[test]
fn a_failed_expectation_does_not_consume_the_offending_token() {
    // `consume` fails at `)` without eating it; `synchronize` skips it and then
    // stops in front of `var`, so the second statement is still checked.
    // (If `)` had been eaten, `synchronize` would swallow `var` as well.)
    assert_eq!(errors("a ) var b = ;"), [
        "[line 1] Error at ')': Expect ';' after expression.",
        "[line 1] Error at ';': Expect expression.",
    ]);
}

#[test]
fn caps_parameters_and_arguments_at_255() {
    let params = |n: usize| {
        (0..n)
            .map(|i| format!("p{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    assert!(
        Parser::new(&format!("fun f({}) {{}}", params(255)))
            .parse_program()
            .is_ok()
    );
    // Reported, not thrown: the whole function still parses, so there is
    // exactly one error. (A thrown one would resume mid-function and trip
    // over its `}`.)
    assert_eq!(errors(&format!("fun f({}) {{ var a = 1; }}", params(256))), ["[line 1] Error at \
                                                                              'p255': Can't \
                                                                              have more than \
                                                                              255 parameters."]);

    let args = |n| vec!["1"; n].join(", ");
    assert!(
        Parser::new(&format!("f({});", args(255)))
            .parse_program()
            .is_ok()
    );

    // Reported, not thrown: the call still parses to its `)`, so the stray `x`
    // after it is found too. (A thrown error would have skipped it.)
    assert_eq!(errors(&format!("f({}) x;", args(256))), [
        "[line 1] Error at '1': Can't have more than 255 arguments.",
        "[line 1] Error at 'x': Expect ';' after expression.",
    ]);
}

#[test]
fn parse_expression_reports_errors_too() {
    let err = |src| match Parser::new(src).parse_expression() {
        Ok(_) => panic!("{src:?} should fail"),
        Err(errs) => errs.iter().map(ToString::to_string).collect::<Vec<_>>(),
    };
    assert_eq!(err("1 +"), ["[line 1] Error at end: Expect expression."]);
    assert_eq!(err(""), ["[line 1] Error at end: Expect expression."]);
    assert_eq!(err("1 2"), ["[line 1] Error at '2': Expect end of expression."]);
    assert_eq!(err("(1 +"), ["[line 1] Error at end: Expect expression."]);
}

/// Any program the book's suite does *not* mark as erroneous must parse with no
/// diagnostics at all: valid programs, runtime-error programs, and the
/// book's benchmarks.
#[test]
fn every_program_not_expected_to_fail_parses_cleanly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files = discover(&root);
    files.sort();

    let mut checked = 0u16;
    let mut failures = Vec::new();

    for path in &files {
        let source = match fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(err) => panic!("cannot read {}: {err}", path.display()),
        };

        if source
            .lines()
            .any(|line| line.contains("// Error") || line.contains("] Error"))
        {
            continue;
        }

        checked += 1;

        if let Err(errs) = Parser::new(&source).parse_program() {
            let shown: Vec<_> = errs.iter().map(ToString::to_string).collect();
            failures.push(format!("{}: {}", path.display(), shown.join(" | ")));
        }
    }

    // Guard against the walk silently finding nothing.
    assert!(checked >= 190, "only {checked} fixture programs were checked");
    assert!(failures.is_empty(), "valid programs rejected:\n{}", failures.join("\n"));
}

/// Fixture directories that hold no ordinary programs:
/// - `limit` (clox's bytecode limits)
/// - `scanning` (token soup)
/// - `expressions` (single expressions, not programs).
const SKIPPED_DIRS: [&str; 3] = ["limit", "scanning", "expressions"];

fn is_skipped_dir(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| SKIPPED_DIRS.iter().any(|skipped| name == *skipped))
}

fn is_lox_file(path: &Path) -> bool { path.extension().is_some_and(|ext| ext == "lox") }

/// Every `.lox` file under `dir`, except those inside [`SKIPPED_DIRS`].
fn discover(dir: &Path) -> Vec<PathBuf> {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|err| panic!("cannot read {}: {err}", dir.display()));

    let (dirs, files): (Vec<_>, Vec<_>) = entries
        .flatten()
        .map(|entry| entry.path())
        .partition(|path| path.is_dir());

    files
        .into_iter()
        .filter(|path| is_lox_file(path))
        .chain(
            dirs.into_iter()
                .filter(|path| !is_skipped_dir(path))
                .flat_map(|path| discover(&path)),
        )
        .collect()
}
