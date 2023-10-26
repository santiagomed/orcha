use std::collections::HashMap;

use crate::prompt::clean_prompt;

use super::chain::LLMChain;
use super::{Chain, ChainResult};
use anyhow::Result;
use tokio::sync::RwLock;

pub struct SequentialChain {
    /// The name of the LLMChain.
    name: String,

    /// Vector of LLM chains used by the SequentialChain.
    chains: Vec<RwLock<LLMChain>>,

    /// The context for for the templates used by the SequentialChain.
    context: HashMap<String, String>,
}

impl Default for SequentialChain {
    fn default() -> Self {
        Self {
            name: uuid::Uuid::new_v4().to_string(),
            chains: Vec::new(),
            context: HashMap::new(),
        }
    }
}

impl SequentialChain {
    /// Initialize a new sequential chain.
    pub fn new() -> SequentialChain {
        SequentialChain::default()
    }

    /// Add a simple LLM Chain to the sequential chain.
    pub fn link(mut self, chain: LLMChain) -> SequentialChain {
        self.chains.push(RwLock::new(chain));
        self
    }
}

pub fn format_prompt_as_user(prompt: &mut str) -> String {
    format!("{{{{#user}}}}{}{{{{/user}}}}", clean_prompt(prompt, true))
}

#[async_trait::async_trait]
impl Chain for SequentialChain {
    async fn execute(&self, target: &str) -> Result<ChainResult> {
        let mut response = String::new();
        let mut result: ChainResult = ChainResult::new(self.name.to_string()); // initialize result to a default value
        for chain in &self.chains {
            if !response.is_empty() {
                chain.write().await.prompt.add_to_template(target, &format_prompt_as_user(&mut response));
            }
            result = chain.read().await.execute(target).await?;
            response = result.content();
        }
        Ok(result)
    }

    fn context(&mut self) -> &mut HashMap<String, String> {
        &mut self.context
    }

    async fn load_context<T>(&mut self, context: &T)
    where
        T: serde::Serialize + Sync,
    {
        for chain in &mut self.chains {
            chain.write().await.load_context(context).await;
        }
    }
}

#[cfg(test)]
mod test {

    use std::sync::Arc;

    use super::*;
    use crate::llm::openai::OpenAI;
    use serde::Serialize;

    #[derive(Serialize)]
    pub struct Data {
        play: String,
    }

    #[tokio::test]
    async fn test_generate() {
        let client = Arc::new(OpenAI::new());

        let first = "{{#chat}}{{#user}}Give me a summary of {{play}}'s plot.{{/user}}{{/chat}}";
        let second = "{{#chat}}{{#system}}You are a professional critic. When given a summary of a play, you must write a review of it. Here is a summary of {{play}}'s plot:{{/system}}{{/chat}}";

        let mut chain = SequentialChain::new()
            .link(LLMChain::new(client.clone()).with_prompt("review", first))
            .link(LLMChain::new(client).with_prompt("review", second));
        chain
            .load_context(&Data {
                play: "Hamlet".to_string(),
            })
            .await;
        let res = chain.execute("review").await;
        assert!(res.is_ok());
    }
}
