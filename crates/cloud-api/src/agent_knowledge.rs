use std::collections::BTreeSet;

use cloud_control::{CloudControlStore, KnowledgeDocument};
use edge_core::RuntimeProtocolCatalog;
use serde::Serialize;
use sha2::{Digest, Sha256};

const DEFAULT_KNOWLEDGE_LIMIT: usize = 6;
const MAX_KNOWLEDGE_LIMIT: usize = 12;
const CHUNK_CHARS: usize = 720;
const CHUNK_OVERLAP_CHARS: usize = 96;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentKnowledgeHit {
    pub document_id: String,
    pub chunk_id: String,
    pub title: String,
    pub source_uri: Option<String>,
    pub source_type: AgentKnowledgeSourceType,
    pub source_revision: String,
    pub project_id: Option<String>,
    pub tags: Vec<String>,
    pub excerpt: String,
    pub score: f64,
    pub content_hash: String,
    pub untrusted_content: bool,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKnowledgeSourceType {
    ManagedDocument,
    RuntimeCatalog,
    BuiltInRunbook,
    ConfigurationSchema,
}

#[derive(Clone, Debug)]
struct SearchDocument {
    document_id: String,
    project_id: Option<String>,
    title: String,
    source_uri: Option<String>,
    source_type: AgentKnowledgeSourceType,
    source_revision: String,
    tags: Vec<String>,
    content: String,
    untrusted_content: bool,
    project_boost: bool,
}

pub fn search_agent_knowledge(
    store: &CloudControlStore,
    query: &str,
    project_id: Option<&str>,
    limit: Option<usize>,
) -> Vec<AgentKnowledgeHit> {
    let terms = search_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let normalized_query = normalize(query);
    let mut hits = knowledge_corpus(store, project_id)
        .into_iter()
        .flat_map(|document| {
            let title = normalize(&document.title);
            let tags = normalize(&document.tags.join(" "));
            let terms = &terms;
            let normalized_query = normalized_query.as_str();
            chunk_text(&safe_knowledge_text(&document.content))
                .into_iter()
                .enumerate()
                .filter_map(move |(index, excerpt)| {
                    let normalized_excerpt = normalize(&excerpt);
                    let score = relevance_score(
                        terms,
                        normalized_query,
                        &title,
                        &tags,
                        &normalized_excerpt,
                        document.project_boost,
                    );
                    (score > 0.0).then(|| AgentKnowledgeHit {
                        document_id: document.document_id.clone(),
                        chunk_id: format!("{}#chunk-{:03}", document.document_id, index + 1),
                        title: document.title.clone(),
                        source_uri: document.source_uri.clone(),
                        source_type: document.source_type,
                        source_revision: document.source_revision.clone(),
                        project_id: document.project_id.clone(),
                        tags: document.tags.clone(),
                        content_hash: content_hash(&excerpt),
                        excerpt,
                        score,
                        untrusted_content: document.untrusted_content,
                    })
                })
        })
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.document_id.cmp(&right.document_id))
            .then_with(|| left.chunk_id.cmp(&right.chunk_id))
    });
    hits.truncate(
        limit
            .unwrap_or(DEFAULT_KNOWLEDGE_LIMIT)
            .clamp(1, MAX_KNOWLEDGE_LIMIT),
    );
    hits
}

fn knowledge_corpus(store: &CloudControlStore, project_id: Option<&str>) -> Vec<SearchDocument> {
    let mut documents = store
        .knowledge_documents()
        .filter(|document| {
            document.enabled
                && (document.project_id.is_none() || document.project_id.as_deref() == project_id)
        })
        .map(|document| managed_document(document, project_id))
        .collect::<Vec<_>>();
    documents.extend(runtime_protocol_documents());
    documents.extend(built_in_documents());
    documents
}

fn managed_document(document: &KnowledgeDocument, project_id: Option<&str>) -> SearchDocument {
    SearchDocument {
        document_id: document.document_id.to_string(),
        project_id: document.project_id.clone(),
        title: document.title.clone(),
        source_uri: document.source_uri.clone(),
        source_type: AgentKnowledgeSourceType::ManagedDocument,
        source_revision: document.updated_at.to_rfc3339(),
        tags: document.tags.clone(),
        content: document.content.clone(),
        untrusted_content: true,
        project_boost: document
            .project_id
            .as_deref()
            .is_some_and(|id| Some(id) == project_id),
    }
}

fn runtime_protocol_documents() -> Vec<SearchDocument> {
    RuntimeProtocolCatalog::all()
        .into_iter()
        .map(|descriptor| SearchDocument {
            document_id: format!("runtime-protocol:{}", descriptor.capability_id),
            project_id: None,
            title: format!("Runtime 协议能力：{}", descriptor.display_name),
            source_uri: Some(format!(
                "velaedge://runtime/protocols/{}",
                descriptor.capability_id
            )),
            source_type: AgentKnowledgeSourceType::RuntimeCatalog,
            source_revision: env!("CARGO_PKG_VERSION").to_owned(),
            tags: vec![
                "工业协议".to_owned(),
                descriptor.capability_id.to_owned(),
                descriptor.display_name.to_owned(),
            ],
            content: format!(
                "协议：{}。能力标识：{}。传输：{:?}。成熟度：{:?}。遥测读取：{}。指令写入：{}。自动探测：{}。实际连接参数必须由协议连接配置提供；下行只允许通过已验证的可写点位和受治理指令编排执行。",
                descriptor.display_name,
                descriptor.capability_id,
                descriptor.transport,
                descriptor.maturity,
                yes_no(descriptor.telemetry_read),
                yes_no(descriptor.command_write),
                yes_no(descriptor.automatic_discovery),
            ),
            untrusted_content: false,
            project_boost: false,
        })
        .collect()
}

fn built_in_documents() -> Vec<SearchDocument> {
    vec![
        built_in(
            "configuration-contract-v1",
            "VelaEdge 产品配置契约与实时同步",
            "velaedge://schema/product-configuration/v1",
            AgentKnowledgeSourceType::ConfigurationSchema,
            &["配置 Schema", "产品版本", "实时同步", "拓扑校验"],
            "产品版本引用点位集，并包含协议连接、采集任务、计算 DSL、采集编排、指令编排和 MQTT 数据源。采集编排必须形成从点位输入到计算节点再到一个或多个输出节点的无环拓扑；指令编排只能连接可写点位。保存前必须校验引用、协议地址、拓扑、数据类型和安全策略。有效配置通过确定性配置 API 生成新的 EdgeConfigPackage 版本，并实时通知在线 Runtime；离线 Runtime 在重连时补偿同步。",
        ),
        built_in(
            "runtime-diagnostics-v1",
            "Runtime 采集与协议故障诊断手册",
            "velaedge://runbooks/runtime-diagnostics/v1",
            AgentKnowledgeSourceType::BuiltInRunbook,
            &["Runtime", "协议超时", "点位质量", "采集"],
            "诊断采集中断时，应关联 Runtime 健康、配置版本、协议连接状态、延迟、超时、重连、熔断状态、采集成功率、坏点数量和最近事件。配置版本不一致时先确认实时同步结果。协议已连接但采集失败时检查点位地址、数据类型、站号或机架槽位等协议参数。结论必须引用指标时间戳，不能把陈旧快照描述为实时状态。",
        ),
        built_in(
            "mqtt-delivery-v1",
            "MQTT 连接与数据交付诊断手册",
            "velaedge://runbooks/mqtt-delivery/v1",
            AgentKnowledgeSourceType::BuiltInRunbook,
            &["MQTT 3.1.1", "MQTT 5.0", "上报", "遗嘱", "QoS"],
            "MQTT 诊断需要同时检查配置与 Runtime 会话：协议版本、Broker、Client ID、Keep Alive、会话参数、TLS、认证是否已配置、连接代次、成功与失败计数、确认延迟、最后主题和最后错误。Topic 与触发频率由采集编排的输出节点定义。MQTT 5.0 可配置会话过期、Receive Maximum、Maximum Packet Size、Topic Alias、用户属性及遗嘱属性。不得在诊断输出中暴露密码、令牌、私钥或其原文。",
        ),
        built_in(
            "command-governance-v1",
            "工业指令下发安全治理手册",
            "velaedge://runbooks/command-governance/v1",
            AgentKnowledgeSourceType::BuiltInRunbook,
            &["指令编排", "可写点位", "幂等", "审计", "风险确认"],
            "自然语言只能生成非执行的指令候选。执行前必须解析 MQTT 输入映射，验证 DeviceSpec 与目标点位可写性，检查类型、范围、来源白名单、速率限制、风险等级、幂等键和人工确认。LLM 不得直接访问协议驱动、寄存器或 PLC 地址；最终写入必须由确定性 Runtime 适配器执行并记录审计与设备回执。",
        ),
    ]
}

fn built_in(
    document_id: &str,
    title: &str,
    source_uri: &str,
    source_type: AgentKnowledgeSourceType,
    tags: &[&str],
    content: &str,
) -> SearchDocument {
    SearchDocument {
        document_id: document_id.to_owned(),
        project_id: None,
        title: title.to_owned(),
        source_uri: Some(source_uri.to_owned()),
        source_type,
        source_revision: env!("CARGO_PKG_VERSION").to_owned(),
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        content: content.to_owned(),
        untrusted_content: false,
        project_boost: false,
    }
}

fn relevance_score(
    terms: &BTreeSet<String>,
    normalized_query: &str,
    title: &str,
    tags: &str,
    excerpt: &str,
    project_boost: bool,
) -> f64 {
    let mut score = if project_boost { 2.0 } else { 0.0 };
    if normalized_query.chars().count() >= 2 && excerpt.contains(normalized_query) {
        score += 12.0;
    }
    for term in terms {
        if title.contains(term) {
            score += 7.0;
        }
        if tags.contains(term) {
            score += 4.0;
        }
        score += excerpt.matches(term).count().min(4) as f64;
    }
    score
}

fn search_terms(query: &str) -> BTreeSet<String> {
    const CJK_STOP: &str = "的是了在和与及或一个这那请帮我如何当前进行支持什么哪些";
    let normalized = normalize(query);
    let mut terms = normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2 && term.chars().count() <= 48)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let cjk = normalized
        .chars()
        .filter(|character| {
            ('\u{4e00}'..='\u{9fff}').contains(character) && !CJK_STOP.contains(*character)
        })
        .collect::<Vec<_>>();
    terms.extend(cjk.iter().map(char::to_string));
    terms.extend(cjk.windows(2).map(|pair| pair.iter().collect::<String>()));
    terms
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn chunk_text(content: &str) -> Vec<String> {
    let chars = content.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + CHUNK_CHARS).min(chars.len());
        chunks.push(chars[start..end].iter().collect::<String>());
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(CHUNK_OVERLAP_CHARS);
    }
    chunks
}

fn safe_knowledge_text(content: &str) -> String {
    const SENSITIVE_MARKERS: [&str; 10] = [
        "password",
        "secret",
        "api_key",
        "apikey",
        "access_token",
        "private key",
        "private_key",
        "authorization:",
        "bearer ",
        "client_secret",
    ];
    content
        .lines()
        .filter(|line| {
            let normalized = line.to_lowercase();
            !SENSITIVE_MARKERS
                .iter()
                .any(|marker| normalized.contains(marker))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn content_hash(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    format!("sha256:{digest:x}")
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "支持"
    } else {
        "不支持"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloud_control::{CloudControlStore, KnowledgeDocument, Project};

    #[test]
    fn project_documents_are_isolated_and_global_protocol_evidence_remains_available() {
        let mut store = CloudControlStore::default();
        store.upsert_project(Project::new("plant-a", "Plant A"));
        store.upsert_project(Project::new("plant-b", "Plant B"));
        let mut a = KnowledgeDocument::new(
            Some("plant-a".to_owned()),
            "Alpha Runbook",
            "alpha-private-fault code A101",
            "operator-a",
        );
        a.tags = vec!["alpha-private-fault".to_owned()];
        store.upsert_knowledge_document(a);
        let mut b = KnowledgeDocument::new(
            Some("plant-b".to_owned()),
            "Beta Runbook",
            "beta-private-fault code B202",
            "operator-b",
        );
        b.tags = vec!["beta-private-fault".to_owned()];
        store.upsert_knowledge_document(b);

        let hits = search_agent_knowledge(
            &store,
            "alpha-private-fault Siemens S7",
            Some("plant-a"),
            Some(12),
        );
        assert!(hits.iter().any(|hit| hit.title == "Alpha Runbook"));
        assert!(hits
            .iter()
            .any(|hit| hit.source_type == AgentKnowledgeSourceType::RuntimeCatalog));
        assert!(!hits.iter().any(|hit| hit.title == "Beta Runbook"));
    }

    #[test]
    fn results_are_chunk_cited_versioned_and_secret_lines_are_removed() {
        let mut store = CloudControlStore::default();
        let document = KnowledgeDocument::new(
            None,
            "MQTT Troubleshooting",
            "connection timeout checklist\npassword=do-not-return\ncheck keep alive and session expiry",
            "operator",
        );
        store.upsert_knowledge_document(document);
        let hits = search_agent_knowledge(&store, "MQTT timeout keep alive", None, Some(12));
        let managed = hits
            .iter()
            .find(|hit| hit.title == "MQTT Troubleshooting")
            .unwrap();
        assert!(managed.chunk_id.contains("#chunk-"));
        assert!(managed.content_hash.starts_with("sha256:"));
        assert!(managed.score > 0.0);
        assert!(managed.untrusted_content);
        assert!(!managed.excerpt.contains("do-not-return"));
    }
}
