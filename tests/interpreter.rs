//! Evaluator tests: values, scope, control flow, functions, runtime errors, and
//! the guards against runaway programs. (The `fixtures` test checks the book's
//! programs end to end; these pin down behavior the fixtures can't see, like
//! evaluation order and what happens when the output fails.)

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use lox::prelude::{Failure, Interpreter, Parser, with_big_stack};

    /// Runs `src` to the end. Returns what it printed and, if it failed, the
    /// text the command line would show on stderr.
    fn run(src: &str) -> (String, Option<String>) {
        let result = with_big_stack(|| {
            let program = match Parser::new(src).parse() {
                Ok(program) => program,
                Err(errs) => panic!("{src:?} should parse: {errs:?}"),
            };

            let mut out = Vec::new();
            let outcome = Interpreter::new(&mut out).interpret(&program);
            (out, outcome.err().map(|failure| failure.to_string()))
        });

        match result {
            Ok((out, failure)) => (String::from_utf8_lossy(&out).into_owned(), failure),
            Err(err) => panic!("could not start the interpreter thread: {err}"),
        }
    }

    /// What `src` prints; it must run without failing.
    fn printed(src: &str) -> String {
        match run(src) {
            (out, None) => out,
            (out, Some(failure)) => panic!("{src:?} failed after printing {out:?}: {failure}"),
        }
    }

    /// What `src` prints before failing, and the failure it ends with.
    fn fails(src: &str) -> (String, String) {
        match run(src) {
            (out, Some(failure)) => (out, failure),
            (out, None) => panic!("{src:?} should fail, but printed {out:?}"),
        }
    }

    #[test]
    fn values_print_like_the_book() {
        let cases = [
            ("print 2;", "2"),
            ("print 2.5;", "2.5"),
            ("print 0.1 + 0.2;", "0.30000000000000004"),
            ("print -0;", "-0"),
            ("print 123456789;", "123456789"),
            ("print 0 / 0;", "NaN"),
            ("print 1 / 0;", "Infinity"),
            ("print -1 / 0;", "-Infinity"),
            ("print \"hi\";", "hi"),
            ("print \"\";", ""),
            ("print nil;", "nil"),
            ("print true;", "true"),
            ("print false;", "false"),
            ("fun f() {} print f;", "<fn f>"),
            ("print clock;", "<native fn>"),
        ];
        for (src, want) in cases {
            assert_eq!(printed(src), format!("{want}\n"), "source: {src}");
        }
    }

    #[test]
    fn arithmetic_comparison_and_strings() {
        let cases = [
            ("print 2 + 3 * 4 - 6 / 3;", "12"),
            ("print (2 + 3) * 4;", "20"),
            ("print -(-3);", "3"),
            ("print 1 < 2;", "true"),
            ("print 2 <= 2;", "true"),
            ("print 3 > 4;", "false"),
            ("print 3 >= 4;", "false"),
            ("print \"foo\" + \"bar\";", "foobar"),
            ("print !true;", "false"),
            ("print !nil;", "true"),
            ("print !!0;", "true"),
        ];
        for (src, want) in cases {
            assert_eq!(printed(src), format!("{want}\n"), "source: {src}");
        }
    }

    #[test]
    fn only_nil_and_false_are_falsey() {
        for (cond, want) in [
            ("nil", "no"),
            ("false", "no"),
            ("true", "yes"),
            ("0", "yes"),
            ("\"\"", "yes"),
            ("clock", "yes"),
        ] {
            let src = format!("if ({cond}) print \"yes\"; else print \"no\";");
            assert_eq!(printed(&src), format!("{want}\n"), "condition: {cond}");
        }
    }

    #[test]
    fn equality_has_no_implicit_conversions() {
        let cases = [
            ("nil == nil", "true"),
            ("nil == false", "false"),
            ("0 == false", "false"),
            ("1 == \"1\"", "false"),
            ("\"a\" + \"b\" == \"ab\"", "true"),
            ("1 != 2", "true"),
            // NaN is not equal to anything, itself included.
            ("(0 / 0) == (0 / 0)", "false"),
            ("(0 / 0) != (0 / 0)", "true"),
            ("clock == clock", "true"),
        ];
        for (expr, want) in cases {
            assert_eq!(printed(&format!("print {expr};")), format!("{want}\n"), "expr: {expr}");
        }
        // functions are equal only to themselves, not to another with the same
        // body
        assert_eq!(
            printed("fun f() {} fun g() {} print f == f; print f == g; var h = f; print h == f;"),
            "true\nfalse\ntrue\n"
        );
        // each *evaluation* of a declaration creates a new function
        assert_eq!(
            printed("fun make() { fun f() {} return f; } print make() == make();"),
            "false\n"
        );
    }

    #[test]
    fn logical_operators_return_an_operand_and_short_circuit() {
        assert_eq!(
            printed(
                "print nil or \"a\"; print \"a\" and \"b\"; print nil and \"b\"; print 1 or 2;"
            ),
            "a\nb\nnil\n1\n"
        );
        // the right side is not evaluated at all when the left decides the
        // result
        assert_eq!(
            printed("print false and boom; print true or boom; print nil and boom;"),
            "false\ntrue\nnil\n"
        );
    }

    #[test]
    fn variables_and_scope() {
        assert_eq!(printed("var a; print a;"), "nil\n");
        assert_eq!(printed("var a = 1; print a = 2; print a;"), "2\n2\n");
        assert_eq!(printed("var a = 1; var b = 2; a = b = 3; print a; print b;"), "3\n3\n");
        // a block's own variable shadows the outer one and then disappears
        assert_eq!(
            printed("var a = \"outer\"; { var a = \"inner\"; print a; } print a;"),
            "inner\nouter\n"
        );
        // assignment reaches the variable that already exists, wherever it is
        assert_eq!(printed("var a = 1; { a = 2; } print a;"), "2\n");
        // redefining a global is allowed
        assert_eq!(printed("var a = 1; var a = 2; print a;"), "2\n");
    }

    #[test]
    fn control_flow() {
        assert_eq!(printed("if (true) print 1; else print 2;"), "1\n");
        assert_eq!(printed("if (false) print 1; else print 2;"), "2\n");
        assert_eq!(printed("if (false) print 1;"), "");
        assert_eq!(printed("var i = 0; while (i < 3) { print i; i = i + 1; }"), "0\n1\n2\n");
        assert_eq!(printed("for (var i = 0; i < 3; i = i + 1) print i;"), "0\n1\n2\n");
        // dangling else binds to the nearest if
        assert_eq!(printed("if (true) if (false) print 1; else print 2;"), "2\n");
        // the loop variable lives in the loop's own scope
        let (_, failure) = fails("for (var i = 0; i < 1; i = i + 1) {} print i;");
        assert_eq!(failure, "Undefined variable 'i'.\n[line 1]");
        // `for` pieces are all optional
        assert_eq!(
            printed(
                "fun f() { var n = 0; for (;;) { n = n + 1; if (n == 3) return n; } } print f();"
            ),
            "3\n"
        );
    }

    #[test]
    fn functions_and_return() {
        assert_eq!(printed("fun add(a, b) { return a + b; } print add(1, 2);"), "3\n");
        // falling off the end, or a bare `return`, yields nil
        assert_eq!(printed("fun f() {} print f();"), "nil\n");
        assert_eq!(printed("fun f() { return; } print f();"), "nil\n");
        // `return` leaves the function from inside nested statements and loops
        assert_eq!(printed("fun f() { while (true) { { return \"out\"; } } } print f();"), "out\n");
        assert_eq!(
            printed(
                "fun fib(n) { if (n < 2) return n; return fib(n - 1) + fib(n - 2); } print \
                 fib(20);"
            ),
            "6765\n"
        );
        assert_eq!(printed("fun a() { return b(); } fun b() { return \"b\"; } print a();"), "b\n");
        // a function can be called from a function value in a variable
        assert_eq!(printed("fun f() { return 1; } var g = f; print g();"), "1\n");
        // a `return` inside a callee does not end the caller
        assert_eq!(
            printed("fun inner() { return 1; } fun outer() { inner(); return 2; } print outer();"),
            "2\n"
        );
    }

    #[test]
    fn closures_keep_their_variables() {
        let counter = "
        fun make() { var i = 0; fun count() { i = i + 1; return i; } return count; }
        var a = make();
        var b = make();
        print a(); print a(); print b(); print a();";
        assert_eq!(printed(counter), "1\n2\n1\n3\n");

        // the closure shares the variable; it does not copy its value
        assert_eq!(
            printed("var f; { var a = 1; fun g() { return a; } f = g; a = 2; } print f();"),
            "2\n"
        );
        // two closures over one variable see each other's changes
        let shared = "
        var get; var set;
        { var v = 0; fun g() { return v; } fun s(x) { v = x; } get = g; set = s; }
        set(7); print get();";
        assert_eq!(printed(shared), "7\n");
    }

    #[test]
    fn runtime_errors_have_the_books_messages_and_lines() {
        let cases = [
            ("print 1;\nprint \"a\" - 1;", "1\n", "Operands must be numbers.\n[line 2]"),
            ("-\"x\";", "", "Operand must be a number.\n[line 1]"),
            ("print 1 + \"a\";", "", "Operands must be two numbers or two strings.\n[line 1]"),
            ("print nil + nil;", "", "Operands must be two numbers or two strings.\n[line 1]"),
            ("print \"a\" < \"b\";", "", "Operands must be numbers.\n[line 1]"),
            ("print 1;\nprint nope;", "1\n", "Undefined variable 'nope'.\n[line 2]"),
            ("nope = 1;", "", "Undefined variable 'nope'.\n[line 1]"),
            ("var a = 1;\n\na();", "", "Can only call functions and classes.\n[line 3]"),
            ("\"str\"();", "", "Can only call functions and classes.\n[line 1]"),
            ("fun f(a, b) {}\nf(1);", "", "Expected 2 arguments but got 1.\n[line 2]"),
            ("fun f() {}\nf(1, 2, 3);", "", "Expected 0 arguments but got 3.\n[line 2]"),
            ("clock(1);", "", "Expected 0 arguments but got 1.\n[line 1]"),
            // a failure deep inside a call reports the line where it happened
            ("fun f() {\n  return -\"x\";\n}\nf();", "", "Operand must be a number.\n[line 2]"),
            // a multi-line string must not throw the line count off
            ("var a = \"1\n2\n3\n\";\nerr;", "", "Undefined variable 'err'.\n[line 5]"),
        ];
        for (src, want_out, want_failure) in cases {
            let (out, failure) = fails(src);
            assert_eq!(out, want_out, "output before failing, source: {src:?}");
            assert_eq!(failure, want_failure, "source: {src:?}");
        }
    }

    #[test]
    fn evaluation_order_matches_the_book() {
        let order = "fun p(x) { print x; return x; }";

        // both operands are evaluated, left first, *before* their types are
        // checked
        let (out, failure) = fails(&format!("{order} p(\"L\") - p(\"R\");"));
        assert_eq!(out, "L\nR\n");
        assert_eq!(failure, "Operands must be numbers.\n[line 1]");

        // arguments are evaluated before the callee is checked for being
        // callable...
        let (out, failure) = fails(&format!("{order} 1(p(\"arg\"));"));
        assert_eq!(out, "arg\n");
        assert_eq!(failure, "Can only call functions and classes.\n[line 1]");

        // ...and before the arity is checked
        let (out, failure) = fails(&format!("{order} fun f() {{}} f(p(\"arg\"));"));
        assert_eq!(out, "arg\n");
        assert_eq!(failure, "Expected 0 arguments but got 1.\n[line 1]");

        // arguments run left to right
        assert_eq!(
            printed(&format!("{order} fun f(a, b, c) {{}} f(p(1), p(2), p(3));")),
            "1\n2\n3\n"
        );
    }

    #[test]
    fn runaway_recursion_is_an_error_not_a_crash() {
        let (_, failure) = fails("fun f() { f(); }\nf();");
        assert_eq!(failure, "Stack overflow.\n[line 1]");

        // legitimately deep (but not runaway) recursion still works
        assert_eq!(
            printed("fun f(n) { if (n == 0) return 0; return 1 + f(n - 1); } print f(9000);"),
            "9000\n"
        );

        // Frames that are very heavy (every call buried in nested blocks and
        // parentheses) use far more native stack per call than the call count
        // suggests; the stack guard must still stop them before the stack runs
        // out.
        let nest = |n: usize| ("{ ".repeat(n), "} ".repeat(n), "(".repeat(n), ")".repeat(n));
        let (open_blocks, close_blocks, open_parens, close_parens) = nest(150);
        let heavy = format!(
            "fun f() {open_blocks}return {open_parens}1 + f(){close_parens}; {close_blocks}\nf();"
        );
        let (_, failure) = fails(&heavy);
        assert_eq!(failure, "Stack overflow.\n[line 1]");
    }

    #[test]
    fn an_early_exit_restores_the_scope() {
        // The failure happens inside a block inside a call; a later top-level
        // statement still has to see the global, not the abandoned local scope.
        let src = "
        var a = \"global\";
        fun f() { var a = \"local\"; { var a = \"deeper\"; return a; } }
        print f();
        print a;";
        assert_eq!(printed(src), "deeper\nglobal\n");
    }

    #[test]
    fn top_level_return_just_ends_the_program() {
        // (A resolver will reject this at compile time later; until then it
        // must not crash.)
        assert_eq!(printed("print 1; return; print 2;"), "1\n");
    }

    #[test]
    fn clock_is_a_number() {
        assert_eq!(printed("print clock() > 0; print clock() - clock() <= 0;"), "true\ntrue\n");
    }

    /// A writer that always fails, like a closed pipe.
    struct Closed;

    impl Write for Closed {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }

        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }

    #[test]
    fn a_failing_output_stops_the_program_instead_of_spinning() {
        // If output errors were ignored, this would loop forever.
        let outcome = with_big_stack(|| {
            let src = "while (true) print 1;";
            let program = match Parser::new(src).parse() {
                Ok(program) => program,
                Err(errs) => panic!("should parse: {errs:?}"),
            };
            Interpreter::new(Closed).interpret(&program)
        });
        match outcome {
            Ok(Err(Failure::Output(err))) => assert_eq!(err.kind(), io::ErrorKind::BrokenPipe),
            other => panic!("expected an output failure, got {other:?}"),
        }
    }

    #[test]
    fn evaluate_runs_a_single_expression() {
        let shown = with_big_stack(|| {
            let expr = match Parser::new("(5 - (3 - 1)) + -1").parse_expression() {
                Ok(expr) => expr,
                Err(errs) => panic!("should parse: {errs:?}"),
            };
            Interpreter::new(Vec::new())
                .evaluate(&expr)
                .map(|value| value.to_string())
        });
        match shown {
            Ok(Ok(text)) => assert_eq!(text, "2"),
            other => panic!("unexpected result: {other:?}"),
        }
    }
}
