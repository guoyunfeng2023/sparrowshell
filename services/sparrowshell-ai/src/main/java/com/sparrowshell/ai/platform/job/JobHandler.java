package com.sparrowshell.ai.platform.job;

public interface JobHandler {
    String type();
    void execute(BackgroundJob job) throws Exception;
}
