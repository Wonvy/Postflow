//! 常见 AI 服务的预设。都是 OpenAI 兼容接口，区别只在地址和获取密钥的地方。
//!
//! 模型名称更新很快，这里只给少数长期稳定的别名作默认值，其余在添加时从服务的模型列表里选。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    /// 推荐的默认模型，空字符串表示让用户从列表里选。
    pub model: &'static str,
    /// 在哪里申请 API Key。
    pub key_url: &'static str,
    /// 本机运行的服务，不需要密钥，内容不出这台电脑。
    pub local: bool,
    /// 服务商说要下线的模型，和建议换成的模型。界面上提醒用户换。
    pub retiring: &'static [(&'static str, &'static str)],
}

/// 用这个服务的这个模型时，服务商建议换成哪个（模型要下线了）。
pub fn replacement(preset: &str, base_url: &str, model: &str) -> Option<&'static str> {
    let model = model.trim().to_ascii_lowercase();
    let url = base_url.to_ascii_lowercase();
    PRESETS
        .iter()
        .filter(|p| {
            p.id == preset || (!p.local && url.starts_with(&p.base_url.to_ascii_lowercase()))
        })
        .flat_map(|p| p.retiring.iter())
        .find(|(old, _)| *old == model)
        .map(|(_, new)| *new)
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "deepseek",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com",
        model: "deepseek-chat",
        key_url: "https://platform.deepseek.com/api_keys",
        local: false,
        retiring: &[],
    },
    Preset {
        id: "qwen",
        name: "通义千问",
        base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        model: "qwen-plus",
        key_url: "https://bailian.console.aliyun.com/",
        local: false,
        retiring: &[],
    },
    Preset {
        id: "qwen_intl",
        name: "Qwen (International)",
        base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        model: "qwen-plus",
        key_url: "https://modelstudio.console.alibabacloud.com/",
        local: false,
        retiring: &[],
    },
    Preset {
        id: "gemini",
        name: "Gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        model: "",
        key_url: "https://aistudio.google.com/apikey",
        local: false,
        retiring: &[],
    },
    Preset {
        id: "openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        model: "",
        key_url: "https://platform.openai.com/api-keys",
        local: false,
        retiring: &[],
    },
    Preset {
        id: "moonshot",
        name: "Kimi",
        base_url: "https://api.moonshot.cn/v1",
        model: "",
        key_url: "https://platform.moonshot.cn/console/api-keys",
        local: false,
        retiring: &[],
    },
    Preset {
        id: "zhipu",
        name: "智谱 GLM",
        base_url: "https://open.bigmodel.cn/api/paas/v4",
        // 智谱的文档建议用 GLM-4.7：GLM-4.5 系列即将下线，GLM-5.3 关不掉思考，做总结又慢又费
        model: "glm-4.7",
        key_url: "https://open.bigmodel.cn/usercenter/apikeys",
        local: false,
        retiring: &[("glm-4.5", "glm-4.7"), ("glm-4.5-x", "glm-4.7")],
    },
    Preset {
        id: "siliconflow",
        name: "硅基流动",
        base_url: "https://api.siliconflow.cn/v1",
        model: "",
        key_url: "https://cloud.siliconflow.cn/account/ak",
        local: false,
        retiring: &[],
    },
    Preset {
        id: "openrouter",
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        model: "",
        key_url: "https://openrouter.ai/keys",
        local: false,
        retiring: &[],
    },
    Preset {
        id: "ollama",
        name: "Ollama",
        base_url: "http://localhost:11434/v1",
        model: "",
        key_url: "https://ollama.com/download",
        local: true,
        retiring: &[],
    },
    Preset {
        id: "lmstudio",
        name: "LM Studio",
        base_url: "http://localhost:1234/v1",
        model: "",
        key_url: "https://lmstudio.ai/",
        local: true,
        retiring: &[],
    },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_a_replacement_for_retiring_models() {
        let zhipu = "https://open.bigmodel.cn/api/paas/v4";
        assert_eq!(replacement("zhipu", zhipu, "glm-4.5"), Some("glm-4.7"));
        assert_eq!(replacement("zhipu", zhipu, " GLM-4.5-X "), Some("glm-4.7"));
        // 选了「自定义」但填的是智谱的地址
        assert_eq!(
            replacement("custom", &format!("{zhipu}/"), "glm-4.5"),
            Some("glm-4.7")
        );
        assert_eq!(replacement("zhipu", zhipu, "glm-4.5-air"), None);
        assert_eq!(replacement("zhipu", zhipu, "glm-4.7"), None);
        assert_eq!(
            replacement("deepseek", "https://api.deepseek.com", "glm-4.5"),
            None
        );
    }
}
