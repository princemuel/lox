# Lox

This is the Rust implementation of an intepreter for the Lox programming language.

Lox, is a full-featured, efficient scripting language from Robert Nystrom's book, ["Crafting Interpreters"][crafting-interpreters]. Lox compiles Lox source code into a bytecode format that is executed by a portable virtual machine.

A more complete description of the features of Lox can be found [here][the-lox-language]

[crafting-interpreters]: https://craftinginterpreters.com/
[the-lox-language]: https://craftinginterpreters.com/the-lox-language.html

## Setup

1. Ensure you have `cargo (1.90+)` installed locally
2. Run `./program.sh` to run the program, which is implemented in `src/main.rs`.
   This command compiles the Rust project, so it might be slow the first time you run it. Subsequent runs will be fast.
