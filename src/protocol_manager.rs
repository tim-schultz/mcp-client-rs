use crate::{Protocol, Tool};
use anyhow::Result;
use std::collections::HashMap;

// Change to work with references instead of owned values
fn filter_tools_by_name<'a>(tools: &'a [Tool], tool_names: &[String]) -> Vec<&'a Tool> {
    tools
        .iter()
        .filter(|tool| tool_names.contains(&tool.name))
        .collect()
}

// Work with references instead of owned values
fn format_tools_for_prompt(tools: &[&Tool], starting_index: &usize) -> String {
    tools
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{}. {}: {}\n", i + starting_index, t.name, t.description))
        .collect()
}

pub struct ProtocolManager {
    pub tool_counter: usize,
    pub clients: Vec<Protocol>,
    pub formatted_tools: Vec<String>,
    pub client_tools: HashMap<String, Vec<Tool>>, // This still owns the Tools
}

impl ProtocolManager {
    pub fn new() -> Self {
        Self {
            tool_counter: 1,
            clients: vec![],
            formatted_tools: vec![],
            client_tools: HashMap::new(),
        }
    }

    pub async fn add_protocol(
        &mut self,
        client_id: &str,
        command_args: Vec<&str>,
        tool_names: Option<Vec<String>>,
    ) -> Result<()> {
        let client = Protocol::new(
            "0",
            "npx",
            command_args.iter().map(|&s| s).collect(),
            std::collections::HashMap::new(),
        )
        .await?;

        let tools = client.list_tools().await?;

        // Here we need to clone because we're storing the tools
        let filtered_tools = if let Some(names) = tool_names {
            let refs: Vec<&Tool> = filter_tools_by_name(&tools.tools, &names);
            refs.into_iter().cloned().collect()
        } else {
            tools.tools.clone()
        };

        // Format tools using references
        let refs: Vec<&Tool> = filtered_tools.iter().collect();
        self.formatted_tools
            .push(format_tools_for_prompt(&refs, &self.tool_counter));

        self.tool_counter += filtered_tools.len();
        self.client_tools
            .insert(client_id.to_string(), filtered_tools);
        self.clients.push(client);

        Ok(())
    }

    pub fn get_tools_for_clients(&self, client_ids: Option<&[String]>) -> String {
        let tools: Vec<&Tool> = match client_ids {
            Some(ids) => ids
                .iter()
                .filter_map(|id| self.client_tools.get(id))
                .flat_map(|tools| tools.iter())
                .collect(),
            None => self
                .client_tools
                .values()
                .flat_map(|tools| tools.iter())
                .collect(),
        };

        format_tools_for_prompt(&tools, &1)
    }

    /// Gets tool structs associated with specific client IDs or all tools if no IDs are specified.
    /// This follows the same pattern as get_protocols for consistency across the codebase.
    ///
    /// # Arguments
    /// * `client_ids` - Optional slice of client IDs to filter tools by
    ///
    /// # Returns
    /// * Vec of references to Tool instances matching the filter criteria
    pub fn get_tool_structs<'a>(&'a self, client_ids: Option<&[String]>) -> Vec<&'a Tool> {
        match client_ids {
            Some(ids) => ids
                .iter()
                .filter_map(|id| self.client_tools.get(id))
                .flat_map(|tools| tools.iter())
                .collect(),
            None => self
                .client_tools
                .values()
                .flat_map(|tools| tools.iter())
                .collect(),
        }
    }
    /// Gets protocols associated with specific client IDs or all protocols if no IDs are specified.
    /// Returns references since the protocols need to stay in the ProtocolManager for later use.
    ///
    /// # Arguments
    /// * `client_ids` - Optional slice of client IDs to filter protocols by
    ///
    /// # Returns
    /// * Vec of references to Protocol instances matching the filter criteria
    pub fn get_protocols<'a>(&'a self, client_ids: Option<&[String]>) -> Vec<&'a Protocol> {
        match client_ids {
            Some(ids) => self
                .clients
                .iter()
                .enumerate()
                .filter(|(_i, _)| ids.iter().any(|id| self.client_tools.contains_key(id)))
                .map(|(_, protocol)| protocol)
                .collect(),
            None => self.clients.iter().collect(),
        }
    }
}
// Usage:
//  let mut manager = ProtocolManager::new();

//  manager.add_protocol(
//     vec!["-y", "@modelcontextprotocol/server-github"],
//     Some(vec!["search_repositories".to_string(), "get_file_contents".to_string()])
//  ).await?;

//  manager.add_protocol(
//     vec!["-y", "@modelcontextprotocol/server-filesystem", project_path],
//     None
//  ).await?;

//  manager.add_protocol(
//     vec!["-y", "@modelcontextprotocol/server-sequential-thinking"],
//     None
//  ).await?;
