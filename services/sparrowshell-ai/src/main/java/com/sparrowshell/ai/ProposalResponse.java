package com.sparrowshell.ai;

import java.util.List;
import java.util.Map;

public class ProposalResponse {
    public String requestId;
    public String routeId;
    public String provider;
    public String model;
    public String summary;
    public List<Command> commands;
    public Map<String, Object> usage;

    public static class Command {
        public String command;
        public String riskHint;
        public boolean requiresInteraction;
    }
}
