import { invoke } from "./ipc";
import { AgentId, AgentStatus } from "../types";

export async function checkAgentReady(id: AgentId): Promise<AgentStatus> {
  return await invoke<AgentStatus>("check_agent_ready", { id });
}

export async function checkAgentsReady(): Promise<AgentStatus[]> {
  return await invoke<AgentStatus[]>("check_agents_ready");
}
