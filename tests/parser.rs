#![cfg(test)]

use lox::prelude::{Lexer, Parser};

fn expr(src: &str) -> String {
    match Parser::new(src).parse_expression() {
        Ok(tree) => tree.to_string(),
        Err(err) => panic!("{src:?} should parse as an expression: {err}"),
    }
}

fn program(src: &str) -> Vec<String> {
    match Parser::new(src).parse_program() {
        Ok(stmts) => stmts.iter().map(ToString::to_string).collect(),
        Err(err) => panic!("{src:?} should parse: {err}"),
    }
}

fn error(src: &str) -> String {
    match Parser::new(src).parse_program() {
        Ok(stmts) => panic!("{src:?} should fail to parse, got {} statement(s)", stmts.len()),
        Err(err) => err.to_string(),
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
fn syntax_errors() {
    let cases = [
        ("a + b = c;", "Invalid assignment target."),
        ("(a) = 1;", "Invalid assignment target."),
        ("class F { m() { this = 1; } }", "Invalid assignment target."),
        ("a b;", "Expect ';' after expression."),
        ("else print 1;", "Expect expression."),
        ("for (;;) var a = 1;", "Expect expression."),
        ("var 1 = 2;", "Expect variable name."),
        ("super;", "Expect '.' after 'super'."),
        ("a.1;", "Expect property name after '.'."),
    ];
    for (src, want) in cases {
        let got = error(src);
        assert!(got.starts_with(want), "source: {src}\n  wanted: {want}\n  got:    {got}");
    }
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
    assert!(error(&format!("fun f({}) {{}}", params(256))).starts_with("Can't have more than 255"));

    let args = |n: usize| vec!["1"; n].join(", ");
    assert!(
        Parser::new(&format!("f({});", args(255)))
            .parse_program()
            .is_ok()
    );
    assert!(error(&format!("f({});", args(256))).starts_with("Can't have more than 255"));
}

#[test]
fn tokens_know_their_line() {
    // a string reports the line it *ends* on, like jlox
    let src = "a\n\"x\ny\"\nb // comment\nd";
    let lines: Vec<usize> = Lexer::new(src)
        .filter_map(Result::ok)
        .map(|t| t.line)
        .collect();
    assert_eq!(lines, [1, 3, 4, 5]);
}
