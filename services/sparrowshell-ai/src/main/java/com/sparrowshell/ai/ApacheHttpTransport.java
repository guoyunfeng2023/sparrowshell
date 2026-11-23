package com.sparrowshell.ai;

import org.apache.http.client.methods.CloseableHttpResponse;
import org.apache.http.client.config.RequestConfig;
import org.apache.http.client.methods.HttpPost;
import org.apache.http.entity.StringEntity;
import org.apache.http.impl.client.CloseableHttpClient;
import org.apache.http.impl.client.HttpClients;
import javax.annotation.PreDestroy;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.*;

/** Server-only transport. Its endpoint is supplied solely by DeepSeekProvider. */
final class ApacheHttpTransport implements DeepSeekProvider.HttpTransport {
    private final CloseableHttpClient client;
    private final ExecutorService executor;
    private final int totalTimeoutSeconds;
    private final int maxResponseBytes;

    ApacheHttpTransport(int connectSeconds, int readSeconds, int totalTimeoutSeconds, int maxResponseBytes) {
        RequestConfig config = RequestConfig.custom().setConnectTimeout(connectSeconds * 1000)
                .setConnectionRequestTimeout(5000).setSocketTimeout(readSeconds * 1000).setRedirectsEnabled(false).build();
        this.client = HttpClients.custom().setDefaultRequestConfig(config).disableAutomaticRetries().build();
        this.executor = Executors.newFixedThreadPool(2, runnable -> {
            Thread thread = new Thread(runnable, "deepseek-http");
            thread.setDaemon(true);
            return thread;
        });
        this.totalTimeoutSeconds = totalTimeoutSeconds;
        this.maxResponseBytes = maxResponseBytes;
    }

    public String post(String url, String bearerToken, String body) {
        HttpPost request = new HttpPost(url);
        request.setHeader("Authorization", "Bearer " + bearerToken);
        request.setHeader("Content-Type", "application/json");
        request.setEntity(new StringEntity(body, StandardCharsets.UTF_8));
        Future<String> future = executor.submit(() -> execute(request));
        try { return future.get(totalTimeoutSeconds, TimeUnit.SECONDS); }
        catch (TimeoutException exception) { request.abort(); future.cancel(true); throw new ProviderException("timeout", "模型服务响应超时"); }
        catch (InterruptedException exception) { Thread.currentThread().interrupt(); request.abort(); future.cancel(true); throw new ProviderException("timeout", "模型请求已取消"); }
        catch (ExecutionException exception) {
            Throwable cause = exception.getCause();
            if (cause instanceof ProviderException) throw (ProviderException) cause;
            if (cause instanceof InterruptedIOException) throw new ProviderException("timeout", "模型服务响应超时");
            throw new ProviderException("provider_unavailable", "模型服务暂时不可用");
        }
    }

    private String execute(HttpPost request) throws IOException {
        try (CloseableHttpResponse response = client.execute(request)) {
            int status = response.getStatusLine().getStatusCode();
            if (status == 429) throw new ProviderException("rate_limit", "模型服务已限流，请稍后重试");
            if (status == 400 || status == 401 || status == 403 || status == 404 || status == 422)
                throw new ProviderException("invalid_request", "模型服务配置或请求无效");
            if (status < 200 || status >= 300) throw new ProviderException("provider_unavailable", "模型服务暂时不可用");
            return readBounded(response.getEntity().getContent());
        } catch (InterruptedIOException exception) {
            throw new ProviderException("timeout", "模型服务响应超时");
        } catch (IOException exception) {
            throw new ProviderException("provider_unavailable", "模型服务暂时不可用");
        }
    }

    private String readBounded(InputStream input) throws IOException {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        byte[] buffer = new byte[8192];
        int read;
        while ((read = input.read(buffer)) >= 0) {
            if (output.size() + read > maxResponseBytes) throw new ProviderException("invalid_provider_response", "模型响应超出限制");
            output.write(buffer, 0, read);
        }
        return new String(output.toByteArray(), StandardCharsets.UTF_8);
    }

    @PreDestroy void close() throws IOException { executor.shutdownNow(); client.close(); }
}
