package com.sparrowshell.ai;

import com.fasterxml.jackson.annotation.JsonIgnoreProperties;
import java.util.Collections;
import java.util.Map;

@JsonIgnoreProperties(ignoreUnknown = false)
public class AnalysisRequest {
    public String clientRequestId;
    public String routeId;
    public String locale = "zh-CN";
    public Map<String, Object> context = Collections.emptyMap();
}
