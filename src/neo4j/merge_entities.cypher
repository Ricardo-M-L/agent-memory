MATCH (k:AgentMemoryEntity {namespace: $namespace, name: $keep}),
      (a:AgentMemoryEntity {namespace: $namespace, name: $alias})
WITH k, a
CALL {
    WITH k, a
    MATCH (s:AgentMemoryEntity {namespace: $namespace})-[old:AGENT_MEMORY_RELATION]->(o:AgentMemoryEntity {namespace: $namespace})
    WHERE s = a OR o = a
    WITH old, CASE WHEN s = a THEN k ELSE s END AS src,
              CASE WHEN o = a THEN k ELSE o END AS dst
    // 只改接别名关联边，不影响其他实体或 keep 原有的自环。
    FOREACH (_ IN CASE WHEN src <> dst THEN [1] ELSE [] END |
        MERGE (src)-[r:AGENT_MEMORY_RELATION {predicate: old.predicate}]->(dst)
        ON CREATE SET r = properties(old)
        ON MATCH SET
            r.created_at = CASE WHEN old.created_at < r.created_at THEN old.created_at ELSE r.created_at END,
            r.confidence = CASE WHEN old.confidence > r.confidence THEN old.confidence ELSE r.confidence END,
            r.source_memory_id = coalesce(r.source_memory_id, old.source_memory_id),
            r.invalidated_at = CASE
                WHEN r.invalidated_at IS NULL OR old.invalidated_at IS NULL THEN null
                WHEN old.invalidated_at < r.invalidated_at THEN old.invalidated_at
                ELSE r.invalidated_at END
    )
    DELETE old
    RETURN count(*) AS affected
}
SET k.mention_count = k.mention_count + a.mention_count,
    k.first_seen = CASE WHEN a.first_seen < k.first_seen THEN a.first_seen ELSE k.first_seen END,
    k.last_seen = CASE WHEN a.last_seen > k.last_seen THEN a.last_seen ELSE k.last_seen END,
    k.entity_type = coalesce(k.entity_type, a.entity_type)
// 如仍有外部关系，普通 DELETE 会失败并回滚，保留外部数据。
DELETE a
RETURN affected
