package com.sparrowshell.ai;

import com.alibaba.dashscope.app.Application;
import com.alibaba.dashscope.app.ApplicationOutput;
import com.alibaba.dashscope.app.ApplicationParam;
import com.alibaba.dashscope.app.ApplicationResult;
import com.alibaba.dashscope.app.ApplicationUsage;

/** Small SDK boundary that keeps provider tests independent from the network. */
interface QwenClient {
    Result call(String apiKey, String appId, String modelId, String prompt, String sessionId);

    final class Result {
        public final String text;
        public final String sessionId;
        public final int inputTokens;
        public final int outputTokens;

        public Result(String text, String sessionId, int inputTokens, int outputTokens) {
            this.text = text;
            this.sessionId = sessionId;
            this.inputTokens = inputTokens;
            this.outputTokens = outputTokens;
        }
    }
}

class DashScopeQwenClient implements QwenClient {
    public Result call(String apiKey, String appId, String modelId, String prompt, String sessionId) {
        try {
            ApplicationParam param = ApplicationParam.builder()
                    .apiKey(apiKey)
                    .appId(appId)
                    .modelId(modelId)
                    .prompt(prompt)
                    .sessionId(sessionId)
                    .build();
            ApplicationResult response = new Application().call(param);
            if (response == null) throw new ProviderException("invalid_provider_response", "通义千问返回了空响应");
            Integer status = response.getStatusCode();
            if (status != null && status >= 400) {
                if (status == 429) throw new ProviderException("rate_limit", "模型请求过于频繁，请稍后重试");
                throw new ProviderException("provider_unavailable", "通义千问暂时不可用");
            }
            ApplicationOutput output = response.getOutput();
            int inputTokens = 0;
            int outputTokens = 0;
            ApplicationUsage usage = response.getUsage();
            if (usage != null && usage.getModels() != null) {
                for (ApplicationUsage.ModelUsage model : usage.getModels()) {
                    if (model.getInputTokens() != null) inputTokens += model.getInputTokens();
                    if (model.getOutputTokens() != null) outputTokens += model.getOutputTokens();
                }
            }
            return new Result(output == null ? null : output.getText(),
                    output == null ? null : output.getSessionId(), inputTokens, outputTokens);
        } catch (ProviderException exception) {
            throw exception;
        } catch (Exception exception) {
            throw new ProviderException("provider_unavailable", "通义千问调用失败，请稍后重试");
        }
    }
}
