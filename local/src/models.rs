//! 工具目录（上游 /ai/free 的 `id` 参数 = 62 个工具）
//!
//! 数据来源：`perfectassistant.ai/__manifest?paths=...` 路由表实测提取，
//! 每组 `/tools/<category>/<id>` 即一个上游工具。工具的分类与 id 稳定；
//! 标题/占位符由上游服务端运行时下发（未在静态 chunk 中），此处按 id 派生展示名。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub category: String,
    pub name: String,
    /// 是否为默认工具
    pub is_default: bool,
}

/// 上游全部 62 个工具（分类 → id 列表）
const CATALOG: &[(&str, &[&str])] = &[
    (
        "business",
        &[
            "brainstorm-tool",
            "brand-name-generator",
            "company-profile",
            "interview-questions-generator",
            "keywords-generator",
            "market-research",
            "marketing-campaign-ideas",
            "meeting-minutes-generator",
            "ocr-text",
            "okr-generator",
            "review-generator",
            "sop-generator",
            "startup-ideas",
            "write-cover-letter",
            "write-resume",
        ],
    ),
    (
        "content",
        &[
            "blog-post-brief",
            "blog-post-generator",
            "blog-post-intro",
            "continue-sentence",
            "create-faq",
            "cta-generator",
            "fix-grammar",
            "give-definition",
            "headline-generator",
            "humanize-text",
            "improve",
            "paraphrase",
            "seo-title-and-description",
            "shorter",
            "simplify",
            "speech-writer",
            "summarize",
            "title",
            "translate",
            "write-paragraph",
        ],
    ),
    (
        "social media",
        &[
            "blog-post-to-tweet-thread",
            "content-calendar",
            "instagram-captions",
            "social-media-post-ideas",
            "tiktok-instagram-reel-script",
            "tiktok-or-instagram-reel-ideas",
            "tweet-ideas",
        ],
    ),
    (
        "mail",
        &[
            "angry-customer-email",
            "mail-improve-draft",
            "mail-reply",
            "mail-subject-line-creator",
            "negative-reply-customer-email",
            "positive-reply-customer-email",
        ],
    ),
    (
        "chat",
        &["apology", "birthday", "greeting", "invitation", "reply-chat"],
    ),
    ("advertisement", &["facebook-ads", "google-ads"]),
    ("slides", &["slides-outline", "text-to-slide"]),
    (
        "spreadsheets",
        &["explain-excel-formula", "generate-excel-formula"],
    ),
    (
        "video",
        &["video-script-generator", "youtube-titles-generator"],
    ),
    ("advanced", &["query-chat-gpt"]),
];

/// 把 `brainstorm-tool` 派生成 `Brainstorm Tool`
pub fn prettify(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    models: Vec<Model>,
    by_id: std::collections::HashMap<String, usize>,
}

impl ModelRegistry {
    pub fn new(default_model: &str) -> Self {
        let mut models = Vec::new();
        for (category, ids) in CATALOG {
            for id in *ids {
                models.push(Model {
                    id: (*id).to_string(),
                    category: (*category).to_string(),
                    name: prettify(id),
                    is_default: *id == default_model,
                });
            }
        }
        // 若默认工具不在目录中，追加一个默认项，避免 default 悬空
        if !models.iter().any(|m| m.is_default) {
            models.insert(
                0,
                Model {
                    id: default_model.to_string(),
                    category: "custom".to_string(),
                    name: prettify(default_model),
                    is_default: true,
                },
            );
        }
        let by_id = models
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id.clone(), i))
            .collect();
        Self { models, by_id }
    }

    pub fn all(&self) -> &[Model] {
        &self.models
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
    }

    pub fn get(&self, id: &str) -> Option<&Model> {
        self.by_id.get(id).map(|i| &self.models[*i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_62_tools() {
        let r = ModelRegistry::new("brainstorm-tool");
        assert_eq!(r.len(), 62, "上游工具数应为 62");
    }

    #[test]
    fn catalog_ids_are_unique() {
        let r = ModelRegistry::new("brainstorm-tool");
        let mut ids: Vec<_> = r.all().iter().map(|m| m.id.clone()).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "工具 id 不应重复");
    }

    #[test]
    fn known_ids_present() {
        let r = ModelRegistry::new("brainstorm-tool");
        for id in ["brainstorm-tool", "summarize", "translate", "write-paragraph"] {
            assert!(r.contains(id), "应包含 {id}");
        }
        // 旧 worker 里编造的 id 不应存在
        for bad in ["social-media-post", "essay-writer", "paragraph-writer", "email-writer"] {
            assert!(!r.contains(bad), "不应包含编造 id {bad}");
        }
    }

    #[test]
    fn prettify_works() {
        assert_eq!(prettify("brainstorm-tool"), "Brainstorm Tool");
        assert_eq!(prettify("seo-title-and-description"), "Seo Title And Description");
    }
}
