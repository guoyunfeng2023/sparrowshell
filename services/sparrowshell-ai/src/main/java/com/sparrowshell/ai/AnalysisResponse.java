package com.sparrowshell.ai;

import java.util.Map;

public class AnalysisResponse {
    public String requestId;
    public String routeId;
    public String provider;
    public String model;
    public String analysis;
    public Map<String, Object> usage;
}
