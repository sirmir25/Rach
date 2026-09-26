//! Throughput of the three pipeline stages (`lexer` -> `parser` -> `interpreter`) on
//! representative programs: a small script, a recursive numeric workload (stresses user-function
//! call overhead), and a struct/`impl`-method workload (stresses the method-dispatch and
//! `self`-mutation write-back path added alongside these benches).

use criterion::{criterion_group, criterion_main, Criterion};
use rach::{interpreter, lexer, parser};

const SMALL_SCRIPT: &str = r#"
name = "rach"
greeting = f"hello, {name}"
print(greeting)
total = 0
i = 0
while i < 50:
    total = total + i
    i = i + 1
"#;

const FIBONACCI: &str = r#"
rach fib(n):
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)
end

result = fib(18)
"#;

const STRUCT_METHODS: &str = r#"
struct Point { x, y }

impl Point:
    rach move(self, dx, dy):
        self.x = self.x + dx
        self.y = self.y + dy
    end
end

p = Point { x: 0, y: 0 }
i = 0
while i < 200:
    p.move(1, 1)
    i = i + 1
"#;

fn run(src: &str) {
    let tokens = lexer::tokenize(src).expect("lex");
    let program = parser::parse(tokens).expect("parse");
    interpreter::run(&program, src, "<bench>").expect("run");
}

fn bench_lex_and_parse(c: &mut Criterion) {
    c.bench_function("lex_and_parse_small_script", |b| {
        b.iter(|| {
            let tokens = lexer::tokenize(SMALL_SCRIPT).expect("lex");
            parser::parse(tokens).expect("parse")
        });
    });
}

fn bench_fibonacci(c: &mut Criterion) {
    c.bench_function("run_fib_18_recursive", |b| {
        b.iter(|| run(FIBONACCI));
    });
}

fn bench_struct_methods(c: &mut Criterion) {
    c.bench_function("run_struct_method_loop", |b| {
        b.iter(|| run(STRUCT_METHODS));
    });
}

criterion_group!(benches, bench_lex_and_parse, bench_fibonacci, bench_struct_methods);
criterion_main!(benches);
