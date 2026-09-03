//! agent-memory 命令行工具。
//!
//! 用法：
//! ```text
//! agent-memory <command> [args...]
//!
//! 命令：
//!   add <scope> <key> <type> <text...>      写入一条记忆
//!   recall <scope> <key> <query...>         检索（按 时效×相关×重要 打分）
//!   list <scope> <key> [type]               列出有效记忆
//!   forget <id>                             删除一条记忆
//!   supersede <scope> <key> <id> <text...>  用新内容取代旧记忆
//!   prune                                   清理过期记忆
//!   consolidate <scope> <key> [threshold]   语义去重合并
//!   stats <scope> <key>                     统计
//!   graph add <s> <p> <o>                   新增关系三元组（多值）
//!   graph replace <s> <p> <o>               替换单值关系（旧边失效可追溯）
//!   graph neighbors <entity> [depth]        查实体邻居（默认 1 跳）
//!   graph path <from> <to> [depth]          查两实体间多跳路径（默认 3）
//!   graph entities                          列出全部实体
//!   graph edges [--all]                     列出有效边（--all 含已失效）
//!   graph communities                       社区发现（弱连通分量）
//!   graph merge <keep> <alias>              合并别名实体（实体消歧）
//!   graph stats                             图谱统计摘要
//!   demo                                    运行端到端演示
//!   help                                    帮助
//!
//! 数据库路径通过环境变量 AGENT_MEMORY_DB 指定，默认 ./agent-memory.db。
//! ```

use std::process::ExitCode;
use std::str::FromStr;

use agent_memory::types::{MemoryType, Scope};
use agent_memory::{AgentMemory, MemoryStats};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        return ExitCode::SUCCESS;
    }

    let db = std::env::var("AGENT_MEMORY_DB").unwrap_or_else(|_| "agent-memory.db".into());
    let mem = match AgentMemory::open(&db) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("打开数据库失败: {e}");
            return ExitCode::FAILURE;
        }
    };

    match args[0].as_str() {
        "add" => cmd_add(&mem, &args[1..]),
        "recall" => cmd_recall(&mem, &args[1..]),
        "list" => cmd_list(&mem, &args[1..]),
        "forget" => cmd_forget(&mem, &args[1..]),
        "supersede" => cmd_supersede(&mem, &args[1..]),
        "prune" => cmd_prune(&mem),
        "consolidate" => cmd_consolidate(&mem, &args[1..]),
        "stats" => cmd_stats(&mem, &args[1..]),
        "graph" => cmd_graph(&mem, &args[1..]),
        "demo" => cmd_demo(&mem),
        "help" | "-h" | "--help" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("未知命令: {other}\n");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "agent-memory —— 离线优先的 LLM Agent 记忆层（Rust）\n\n\
         用法: agent-memory <command> [args...]\n\n\
         命令:\n\
         \x20 add <scope> <key> <type> <text...>      写入记忆（type: working|episodic|semantic）\n\
         \x20 recall <scope> <key> <query...>         检索（按 时效×相关×重要 打分）\n\
         \x20 list <scope> <key> [type]               列出有效记忆\n\
         \x20 forget <id>                             删除记忆\n\
         \x20 supersede <scope> <key> <id> <text...>  用新内容取代旧记忆\n\
         \x20 prune                                   清理过期记忆\n\
         \x20 consolidate <scope> <key> [threshold]   语义去重合并\n\
         \x20 stats <scope> <key>                     统计\n\
         \x20 graph add <s> <p> <o>                   新增关系三元组（多值）\n\
         \x20 graph replace <s> <p> <o>               替换单值关系（旧边失效）\n\
         \x20 graph neighbors <entity> [depth]        查实体邻居（默认1跳）\n\
         \x20 graph path <from> <to> [depth]          查多跳路径（默认3跳）\n\
         \x20 graph entities                          列出全部实体\n\
         \x20 graph edges [--all]                     列出有效边（--all含失效）\n\
         \x20 graph communities                       社区发现（弱连通分量）\n\
         \x20 graph merge <keep> <alias>              合并别名实体（消歧）\n\
         \x20 graph stats                             图谱统计摘要\n\
         \x20 demo                                    端到端演示\n\n\
         数据库: 环境变量 AGENT_MEMORY_DB 指定，默认 ./agent-memory.db"
    );
}

fn parse_scope(s: &str) -> Option<Scope> {
    Scope::from_str(s).ok()
}

fn parse_type(s: &str) -> Option<MemoryType> {
    MemoryType::from_str(s).ok()
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("错误: {msg}");
    ExitCode::FAILURE
}

fn ok() -> ExitCode {
    ExitCode::SUCCESS
}

fn cmd_add(mem: &AgentMemory, a: &[String]) -> ExitCode {
    if a.len() < 4 {
        return fail("用法: add <scope> <key> <type> <text...>");
    }
    let (scope, key, ty, rest) = (
        parse_scope(&a[0]).unwrap_or_else(|| {
            eprintln!("无效 scope: {}", a[0]);
            std::process::exit(1);
        }),
        &a[1],
        parse_type(&a[2]).unwrap_or_else(|| {
            eprintln!("无效 type: {} (working|episodic|semantic)", a[2]);
            std::process::exit(1);
        }),
        a[3..].join(" "),
    );
    match mem.add(scope, key, ty, &rest) {
        Ok(item) => {
            println!(
                "#{} [{}] {} (importance={:.2})",
                item.id,
                ty.as_str(),
                item.content,
                item.importance
            );
            ok()
        }
        Err(e) => fail(&format!("写入失败: {e}")),
    }
}

fn cmd_recall(mem: &AgentMemory, a: &[String]) -> ExitCode {
    if a.len() < 3 {
        return fail("用法: recall <scope> <key> <query...>");
    }
    let scope = parse_scope(&a[0]).unwrap_or_else(|| {
        eprintln!("无效 scope: {}", a[0]);
        std::process::exit(1);
    });
    let (key, query) = (&a[1], a[2..].join(" "));
    match mem.recall(scope, key, &query) {
        Ok(hits) => {
            if hits.is_empty() {
                println!("（没有命中）");
            }
            for (i, h) in hits.iter().enumerate() {
                println!(
                    "{}. score={:.3} (rel={:.2} rec={:.2} imp={:.2}) #{} [{}] {}",
                    i + 1,
                    h.score,
                    h.relevance,
                    h.recency,
                    h.importance,
                    h.item.id,
                    h.item.memory_type.as_str(),
                    h.item.content
                );
            }
            ok()
        }
        Err(e) => fail(&format!("检索失败: {e}")),
    }
}

fn cmd_list(mem: &AgentMemory, a: &[String]) -> ExitCode {
    if a.len() < 2 {
        return fail("用法: list <scope> <key> [type]");
    }
    let scope = parse_scope(&a[0]).unwrap_or_else(|| {
        eprintln!("无效 scope: {}", a[0]);
        std::process::exit(1);
    });
    let ty = if a.len() >= 3 {
        Some(parse_type(&a[2]).unwrap_or_else(|| {
            eprintln!("无效 type: {}", a[2]);
            std::process::exit(1);
        }))
    } else {
        None
    };
    match mem.list(scope, &a[1], ty) {
        Ok(items) => {
            for it in items {
                println!(
                    "#{} [{}] {} (imp={:.2})",
                    it.id,
                    it.memory_type.as_str(),
                    it.content,
                    it.importance
                );
            }
            ok()
        }
        Err(e) => fail(&format!("读取失败: {e}")),
    }
}

fn cmd_forget(mem: &AgentMemory, a: &[String]) -> ExitCode {
    if a.is_empty() {
        return fail("用法: forget <id>");
    }
    let id: i64 = match a[0].parse() {
        Ok(v) => v,
        Err(_) => return fail("id 必须是整数"),
    };
    match mem.forget(id) {
        Ok(()) => {
            println!("已删除 #{id}");
            ok()
        }
        Err(e) => fail(&format!("删除失败: {e}")),
    }
}

fn cmd_supersede(mem: &AgentMemory, a: &[String]) -> ExitCode {
    if a.len() < 4 {
        return fail("用法: supersede <scope> <key> <id> <text...>");
    }
    let scope = parse_scope(&a[0]).unwrap_or_else(|| {
        eprintln!("无效 scope: {}", a[0]);
        std::process::exit(1);
    });
    let id: i64 = match a[2].parse() {
        Ok(v) => v,
        Err(_) => return fail("id 必须是整数"),
    };
    let new_content = a[3..].join(" ");
    match mem.supersede(scope, &a[1], id, &new_content) {
        Ok(new) => {
            println!(
                "#{old} -> #{new} 已取代，新记忆: {content}",
                old = id,
                new = new.id,
                content = new.content
            );
            ok()
        }
        Err(e) => fail(&format!("取代失败: {e}")),
    }
}

fn cmd_prune(mem: &AgentMemory) -> ExitCode {
    match mem.prune() {
        Ok(n) => {
            println!("清理过期记忆 {n} 条");
            ok()
        }
        Err(e) => fail(&format!("清理失败: {e}")),
    }
}

fn cmd_consolidate(mem: &AgentMemory, a: &[String]) -> ExitCode {
    if a.len() < 2 {
        return fail("用法: consolidate <scope> <key> [threshold]");
    }
    let scope = parse_scope(&a[0]).unwrap_or_else(|| {
        eprintln!("无效 scope: {}", a[0]);
        std::process::exit(1);
    });
    let threshold: f32 = if a.len() >= 3 {
        match a[2].parse() {
            Ok(v) => v,
            Err(_) => return fail("threshold 必须是浮点数"),
        }
    } else {
        0.82
    };
    match mem.consolidate(scope, &a[1], threshold) {
        Ok(n) => {
            println!("语义去重合并 {n} 条");
            ok()
        }
        Err(e) => fail(&format!("整合失败: {e}")),
    }
}

fn cmd_stats(mem: &AgentMemory, a: &[String]) -> ExitCode {
    if a.len() < 2 {
        return fail("用法: stats <scope> <key>");
    }
    let scope = parse_scope(&a[0]).unwrap_or_else(|| {
        eprintln!("无效 scope: {}", a[0]);
        std::process::exit(1);
    });
    match mem.stats(scope, &a[1]) {
        Ok(MemoryStats {
            working,
            episodic,
            semantic,
            superseded,
            total,
        }) => {
            println!(
                "working={working} episodic={episodic} semantic={semantic} superseded={superseded} total={total}"
            );
            ok()
        }
        Err(e) => fail(&format!("统计失败: {e}")),
    }
}

fn cmd_graph(mem: &AgentMemory, a: &[String]) -> ExitCode {
    let sub = match a.first() {
        Some(s) => s.as_str(),
        None => return fail(
            "用法: graph <add|replace|neighbors|path|entities|edges|communities|merge|stats> ...",
        ),
    };
    match sub {
        "add" | "replace" => {
            if a.len() < 4 {
                return fail("用法: graph <add|replace> <subject> <predicate> <object>");
            }
            let (s, p, o) = (&a[1], &a[2], &a[3]);
            let res = if sub == "add" {
                mem.remember_relation(s, p, o)
            } else {
                mem.replace_relation(s, p, o)
            };
            match res {
                Ok(id) => {
                    println!("#{id} {s} -{p}-> {o}");
                    ok()
                }
                Err(e) => fail(&format!("写入关系失败: {e}")),
            }
        }
        "neighbors" => {
            if a.len() < 2 {
                return fail("用法: graph neighbors <entity> [depth]");
            }
            let depth = a.get(2).and_then(|d| d.parse().ok()).unwrap_or(1);
            match mem.graph_neighbors(&a[1], depth) {
                Ok(edges) => {
                    if edges.is_empty() {
                        println!("（没有相关关系）");
                    }
                    for e in edges {
                        println!("#{} {}", e.id, e.display());
                    }
                    ok()
                }
                Err(e) => fail(&format!("查询失败: {e}")),
            }
        }
        "path" => {
            if a.len() < 3 {
                return fail("用法: graph path <from> <to> [max_depth]");
            }
            let depth = a.get(3).and_then(|d| d.parse().ok()).unwrap_or(3);
            match mem.graph_paths(&a[1], &a[2], depth) {
                Ok(paths) => {
                    if paths.is_empty() {
                        println!("（{} 跳内无路径）", depth);
                    }
                    for (i, path) in paths.iter().enumerate() {
                        let chain = path
                            .iter()
                            .map(|e| format!("{} -{}-> {}", e.subject, e.predicate, e.object))
                            .collect::<Vec<_>>()
                            .join("  =>  ");
                        println!("{}. {} ({} 跳)", i + 1, chain, path.len());
                    }
                    ok()
                }
                Err(e) => fail(&format!("路径查询失败: {e}")),
            }
        }
        "entities" => match mem.graph_entities() {
            Ok(ents) => {
                for en in ents {
                    println!("#{} {} (mentions={})", en.id, en.name, en.mention_count);
                }
                ok()
            }
            Err(e) => fail(&format!("读取实体失败: {e}")),
        },
        "edges" => {
            let include_invalid = a.iter().any(|x| x == "--all");
            match mem.graph_edges(include_invalid) {
                Ok(edges) => {
                    for e in edges {
                        let tag = if e.is_valid() {
                            String::new()
                        } else {
                            " [已失效]".to_string()
                        };
                        println!("#{} {} conf={:.2}{}", e.id, e.display(), e.confidence, tag);
                    }
                    ok()
                }
                Err(e) => fail(&format!("读取边失败: {e}")),
            }
        }
        "communities" => match mem.graph_communities() {
            Ok(comms) => {
                for (i, c) in comms.iter().enumerate() {
                    println!("社区{} ({} 个实体): {}", i + 1, c.len(), c.join(", "));
                }
                ok()
            }
            Err(e) => fail(&format!("社区发现失败: {e}")),
        },
        "merge" => {
            if a.len() < 3 {
                return fail("用法: graph merge <保留实体> <被合并别名>");
            }
            match mem.merge_graph_entities(&a[1], &a[2]) {
                Ok(n) => {
                    println!("已把 '{}' 合并进 '{}'，受影响边 {n} 条", a[2], a[1]);
                    ok()
                }
                Err(e) => fail(&format!("实体合并失败: {e}")),
            }
        }
        "stats" => match mem.graph_summary() {
            Ok(s) => {
                println!(
                    "实体 {} | 有效边 {} | 失效边 {} | 社区 {} | 最大社区 {}",
                    s.entities, s.valid_edges, s.invalid_edges, s.communities, s.largest_community
                );
                ok()
            }
            Err(e) => fail(&format!("统计失败: {e}")),
        },
        other => fail(&format!("未知 graph 子命令: {other}")),
    }
}

fn cmd_demo(mem: &AgentMemory) -> ExitCode {
    use agent_memory::types::MemoryType;

    println!("==== agent-memory 端到端演示 ====\n");
    println!("[1] 写入语义事实与情景事件...");
    mem.remember_fact(Scope::User, "dev", "用户喜欢用 Rust 编写 Agent 应用")
        .unwrap();
    mem.remember_fact(Scope::User, "dev", "用户正在准备 AI 工程师面试")
        .unwrap();
    mem.remember_event(Scope::User, "dev", "上周用户完成了知识图谱 Neo4j 的学习")
        .unwrap();
    mem.remember_event(Scope::User, "dev", "用户关注 gRPC 与 HTTP 的性能对比")
        .unwrap();
    mem.add(
        Scope::User,
        "dev",
        MemoryType::Working,
        "当前会话：研究 agent 记忆系统设计",
    )
    .unwrap();
    println!("已写入 5 条记忆\n");

    println!("[2] 检索 '用户喜欢什么语言'...");
    for h in mem.recall(Scope::User, "dev", "用户喜欢什么语言").unwrap() {
        println!("   score={:.3}  {}", h.score, h.item.content);
    }
    println!();

    println!("[3] 冲突解决：'用户喜欢 Go' 取代旧事实...");
    let old = mem.recall(Scope::User, "dev", "Rust").unwrap()[0]
        .item
        .clone();
    mem.supersede(Scope::User, "dev", old.id, "用户喜欢用 Go 编写 Agent 应用")
        .unwrap();
    println!("   #{old} 已标记为 superseded\n", old = old.id);

    println!("[4] 统计:");
    let s = mem.stats(Scope::User, "dev").unwrap();
    println!(
        "   working={} episodic={} semantic={} superseded={} total={}\n",
        s.working, s.episodic, s.semantic, s.superseded, s.total
    );

    println!("[5] 知识图谱：写入实体关系并做多跳推理...");
    mem.remember_relation("用户", "使用", "Rust").unwrap();
    mem.remember_relation("Rust", "适合", "系统编程").unwrap();
    mem.remember_relation("用户", "研究", "知识图谱").unwrap();
    mem.replace_relation("用户", "住在", "北京").unwrap();
    println!("   用户的 2 跳邻居:");
    for e in mem.graph_neighbors("用户", 2).unwrap() {
        println!("     - {}", e.display());
    }
    println!("   路径 用户 -> 系统编程:");
    for path in mem.graph_paths("用户", "系统编程", 2).unwrap() {
        let chain = path
            .iter()
            .map(|e| format!("{} -{}-> {}", e.subject, e.predicate, e.object))
            .collect::<Vec<_>>()
            .join(" => ");
        println!("     * {chain}");
    }
    println!();

    println!("==== 演示结束 ====");
    ok()
}
