# Lox

[![progress-banner](https://backend.codecrafters.io/progress/interpreter/be86d1f9-9cd2-4a4c-a12c-49f0dee67844)](https://app.codecrafters.io/users/codecrafters-bot?r=2qF)

This is the Rust implementation of an intepreter for the Lox programming language.

Lox, is a full-featured, efficient scripting language from Robert Nystrom's book, ["Crafting Interpreters"][crafting-interpreters]. lox compiles Lox source code into a bytecode format that is executed by a portable virtual machine.

A more complete description of the features of Lox can be found [here][the-lox-language]

**Note**: If you're viewing this repo on GitHub, head over to
[codecrafters.io](https://codecrafters.io) to try the challenge, or just read [the book][crafting-interpreters] 😀 Along the way, you'll learn about tokenization, ASTs,
tree-walk interpreters and much more.

[crafting-interpreters]: https://craftinginterpreters.com/
[the-lox-language]: https://craftinginterpreters.com/the-lox-language.html

## Setup

1. Ensure you have `cargo (1.88+)` installed locally
2. Run `./program.sh` to run the program, which is implemented in `src/main.rs`.
   This command compiles the Rust project, so it might be slow the first time you run it. Subsequent runs will be fast.
