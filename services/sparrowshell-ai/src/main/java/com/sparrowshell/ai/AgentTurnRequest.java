package com.sparrowshell.ai;

import com.fasterxml.jackson.annotation.JsonIgnoreProperties;
import java.util.Collections;
import java.util.List;
import java.util.Map;

@JsonIgnoreProperties(ignoreUnknown = false)
public class AgentTurnRequest {
    public String clientRequestId;
    /** Stable across every model/tool turn in one user task. */
    public String taskId;
    public String routeId;
    public String sessionId;
    public String intent;
    public String locale = "zh-CN";
    public List<Message> history = Collections.emptyList();
    public Map<String, Object> context = Collections.emptyMap();

    @JsonIgnoreProperties(ignoreUnknown = false)
    public static class Message {
        public String role;
        public String content;
    }
}
