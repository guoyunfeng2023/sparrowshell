package com.sparrowshell.ai;

import com.fasterxml.jackson.annotation.JsonIgnoreProperties;
import java.util.Collections;
import java.util.Map;

@JsonIgnoreProperties(ignoreUnknown = false)
public class ProposalRequest {
    public String clientRequestId;
    public String routeId;
    public String intent;
    public String locale = "zh-CN";
    public Map<String, Object> context = Collections.emptyMap();
    public Limits limits = new Limits();

    @JsonIgnoreProperties(ignoreUnknown = false)
    public static class Limits {
        public int maxProposals = 3;
    }
}
