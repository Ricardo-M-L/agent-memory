// 调用方已在读取图数据前取得命名空间写锁。
// 预留应用 ID（允许间隙），不依赖 Neo4j 内部 ID。
SET g.next_id = g.next_id + 3
WITH g.next_id AS serial
MERGE (s:AgentMemoryEntity {namespace: $namespace, name: $subject})
ON CREATE SET s.id = serial - 2, s.mention_count = 0,
              s.first_seen = $now, s.last_seen = $now
SET s.mention_count = s.mention_count + 1,
    s.first_seen = CASE WHEN $now < s.first_seen THEN $now ELSE s.first_seen END,
    s.last_seen = CASE WHEN $now > s.last_seen THEN $now ELSE s.last_seen END
MERGE (o:AgentMemoryEntity {namespace: $namespace, name: $object})
ON CREATE SET o.id = serial - 1, o.mention_count = 0,
              o.first_seen = $now, o.last_seen = $now
SET o.mention_count = o.mention_count + 1,
    o.first_seen = CASE WHEN $now < o.first_seen THEN $now ELSE o.first_seen END,
    o.last_seen = CASE WHEN $now > o.last_seen THEN $now ELSE o.last_seen END
WITH s, o, serial
CALL {
    WITH s, o
    MATCH (s)-[old:AGENT_MEMORY_RELATION {predicate: $predicate}]->(other:AgentMemoryEntity {namespace: $namespace})
    WHERE $replace AND other <> o AND old.invalidated_at IS NULL
    SET old.invalidated_at = $now
    RETURN count(old) AS invalidated
}
MERGE (s)-[r:AGENT_MEMORY_RELATION {predicate: $predicate}]->(o)
ON CREATE SET r.id = serial, r.source_memory_id = $source,
              r.confidence = $confidence, r.created_at = $now
// add 保留失效状态，只有 replace 显式恢复旧事实。
ON MATCH SET
    r.source_memory_id = CASE WHEN $replace AND r.invalidated_at IS NOT NULL THEN $source ELSE r.source_memory_id END,
    r.created_at = CASE WHEN $replace AND r.invalidated_at IS NOT NULL THEN $now ELSE r.created_at END,
    r.confidence = CASE
        WHEN $replace AND r.invalidated_at IS NOT NULL THEN $confidence
        WHEN r.invalidated_at IS NULL AND $confidence > r.confidence THEN $confidence
        ELSE r.confidence END
FOREACH (_ IN CASE WHEN $replace THEN [1] ELSE [] END | REMOVE r.invalidated_at)
RETURN r.id
