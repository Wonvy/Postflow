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
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "deepseek",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com",
        model: "deepseek-chat",
        key_url: "https://platform.deepseek.com/api_keys",
        local: false,
    },
    Preset {
        id: "qwen",
        name: "通义千问",
        base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        model: "qwen-plus",
        key_url: "https://bailian.console.aliyun.com/",
        local: false,
    },
    Preset {
        id: "qwen_intl",
        name: "Qwen (International)",
        base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        model: "qwen-plus",
        key_url: "https://modelstudio.console.alibabacloud.com/",
        local: false,
    },
    Preset {
        id: "gemini",
        name: "Gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        model: "",
        key_url: "https://aistudio.google.com/apikey",
        local: false,
    },
    Preset {
        id: "openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        model: "",
        key_url: "https://platform.openai.com/api-keys",
        local: false,
    },
    Preset {
        id: "moonshot",
        name: "Kimi",
        base_url: "https://api.moonshot.cn/v1",
        model: "",
        key_url: "https://platform.moonshot.cn/console/api-keys",
        local: false,
    },
    Preset {
        id: "zhipu",
        name: "智谱 GLM",
        base_url: "https://open.bigmodel.cn/api/paas/v4",
        model: "",
        key_url: "https://open.bigmodel.cn/usercenter/apikeys",
        local: false,
    },
    Preset {
        id: "siliconflow",
        name: "硅基流动",
        base_url: "https://api.siliconflow.cn/v1",
        model: "",
        key_url: "https://cloud.siliconflow.cn/account/ak",
        local: false,
    },
    Preset {
        id: "openrouter",
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        model: "",
        key_url: "https://openrouter.ai/keys",
        local: false,
    },
    Preset {
        id: "ollama",
        name: "Ollama",
        base_url: "http://localhost:11434/v1",
        model: "",
        key_url: "https://ollama.com/download",
        local: true,
    },
    Preset {
        id: "lmstudio",
        name: "LM Studio",
        base_url: "http://localhost:1234/v1",
        model: "",
        key_url: "https://lmstudio.ai/",
        local: true,
    },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}
