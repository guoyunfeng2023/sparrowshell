package com.sparrowshell.ai.platform.admin;

import com.sparrowshell.ai.SparrowShellAiApplication;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.web.servlet.MockMvc;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = SparrowShellAiApplication.class, properties = {"server.port=0", "sparrowshell.platform.persistence-enabled=false"})
@AutoConfigureMockMvc
class AdminUnavailableIntegrationTest {
    @Autowired MockMvc mvc;
    @Test void managementRoutesAreExplicitlyUnavailable() throws Exception {
        mvc.perform(get("/v1/admin/users")).andExpect(status().isUnauthorized()).andExpect(jsonPath("$.error.code").value("authentication_required"));
        mvc.perform(get("/v1/admin/users").header("Authorization", "Bearer invalid")).andExpect(status().isServiceUnavailable()).andExpect(jsonPath("$.error.code").value("unavailable"));
        mvc.perform(get("/v1/admin/ai/overview").header("Authorization", "Bearer invalid")).andExpect(status().isServiceUnavailable()).andExpect(jsonPath("$.error.code").value("unavailable"));
    }
}
