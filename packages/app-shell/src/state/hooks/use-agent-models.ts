import { useQuery } from "@tanstack/react-query";
import { useContractsClient } from "../../contracts-client-context";
import { queryKeys } from "./query-keys";

/** Loads the available agent CLI models through the contracts client and caches them. */
export function useAgentModels() {
  const client = useContractsClient();
  return useQuery({
    queryKey: queryKeys.agentModels,
    queryFn: () => client.agentRuntime.listModels({}).then((response) => response.groups),
  });
}
