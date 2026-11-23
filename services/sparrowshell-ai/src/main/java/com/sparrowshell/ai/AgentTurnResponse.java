package com.sparrowshell.ai;

import java.util.Map;

public class AgentTurnResponse {
    public String requestId;
    public String routeId;
    public String provider;
    public String model;
    public String sessionId;
    public String type;
    public ToolCall toolCall;
    public FinalAnswer finalAnswer;
    public Map<String, Object> usage;

    public static class ToolCall {
        public String summary;
        public String command;
    }

    public static class FinalAnswer {
        public String answer;
    }
}
