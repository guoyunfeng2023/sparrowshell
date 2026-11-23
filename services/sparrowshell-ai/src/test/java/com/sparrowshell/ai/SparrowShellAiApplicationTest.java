package com.sparrowshell.ai;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import java.time.Instant;
import static org.junit.jupiter.api.Assertions.assertEquals;

@SpringBootTest(properties = {"server.port=0", "sparrowshell.deepseek.api-key="})
class SparrowShellAiApplicationTest {
    @Autowired ObjectMapper objectMapper;

    @Test void applicationStartsWithoutCallingProvider() { }

    @Test void strictObjectMapperSerializesJavaTimeAsIso8601() throws Exception {
        assertEquals("\"2026-11-04T01:02:03Z\"",
                objectMapper.writeValueAsString(Instant.parse("2026-11-04T01:02:03Z")));
    }
}
