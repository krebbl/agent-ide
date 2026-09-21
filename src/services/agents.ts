import { invoke } from "./ipc";
import { AgentId, AgentModel, AgentStatus, AgentConversationSummary } from "../types";

export async function checkAgentReady(id: AgentId): Promise<AgentStatus> {
  return await invoke<AgentStatus>("check_agent_ready", { id });
}

export async function checkAgentsReady(): Promise<AgentStatus[]> {
  return await invoke<AgentStatus[]>("check_agents_ready");
}

export async function listAgentModels(id: AgentId): Promise<AgentModel[]> {
  return await invoke<AgentModel[]>("list_agent_models", { id });
}

export async function buildAgentCommand(
  agentId: AgentId,
  model: string | null,
  prompt: string,
): Promise<string[]> {
  return await invoke<string[]>("build_agent_command", {
    agentId,
    model,
    prompt,
  });
}

export async function listRecentAgentConversations(
  cwd: string,
): Promise<AgentConversationSummary[]> {
  return await invoke<AgentConversationSummary[]>("recent_agent_sessions", { cwd });
}

export async function buildAgentResumeCommand(
  agentId: string,
  conversationId: string,
): Promise<string[]> {
  return await invoke<string[]>("resume_agent_command", {
    agentId,
    conversationId,
  });
}
