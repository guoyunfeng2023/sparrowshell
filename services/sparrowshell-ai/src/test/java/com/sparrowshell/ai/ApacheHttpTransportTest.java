package com.sparrowshell.ai;

import com.sun.net.httpserver.HttpServer;
import org.junit.jupiter.api.Test;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.atomic.AtomicInteger;
import static org.junit.jupiter.api.Assertions.*;

class ApacheHttpTransportTest {
    @Test void releasesPooledConnectionsAfterProviderErrors() throws Exception {
        AtomicInteger calls = new AtomicInteger();
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        server.createContext("/chat/completions", exchange -> {
            int call = calls.incrementAndGet();
            byte[] body = (call < 3 ? "provider error" : "{}").getBytes(StandardCharsets.UTF_8);
            exchange.getResponseHeaders().set("Content-Type", "application/json");
            exchange.sendResponseHeaders(call < 3 ? 401 : 200, body.length);
            exchange.getResponseBody().write(body);
            exchange.close();
        });
        server.start();
        ApacheHttpTransport transport = new ApacheHttpTransport(1, 5, 10, 1024);
        String endpoint = "http://127.0.0.1:" + server.getAddress().getPort() + "/chat/completions";
        try {
            for (int index = 0; index < 2; index++) {
                ProviderException error = assertThrows(ProviderException.class,
                        () -> transport.post(endpoint, "server-only-secret", "{}"));
                assertEquals("invalid_request", error.code());
            }
            assertEquals("{}", transport.post(endpoint, "server-only-secret", "{}"));
            assertEquals(3, calls.get());
        } finally {
            transport.close();
            server.stop(0);
        }
    }
}
