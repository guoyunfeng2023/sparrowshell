package com.sparrowshell.ai.platform.admin;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.transaction.PlatformTransactionManager;
import java.util.Collections;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

class AdminSubscriptionServiceTest {
    private final AdminRepository repository = mock(AdminRepository.class);
    private final AdminAuthService auth = mock(AdminAuthService.class);
    private final ObjectProvider<PlatformTransactionManager> provider = mock(ObjectProvider.class);
    private final AdminModels.AdminSession session = new AdminModels.AdminSession();
    private AdminService service;

    @BeforeEach void setUp() {
        when(provider.getIfAvailable()).thenReturn(null);
        service = new AdminService(repository, auth, provider);
        session.adminId = "admin-1"; session.email = "admin@example.com"; session.role = "VIEWER";
    }

    @Test void subscriptionsUseReadRoleAndValidatedFilters() {
        AdminModels.BillingSubscription subscription = new AdminModels.BillingSubscription(); subscription.id = "period-1";
        when(repository.billingSubscriptions("device", "active", "high", 50, 0)).thenReturn(Collections.singletonList(subscription));
        assertEquals("period-1", service.billingSubscriptions(session, "device", "active", "high", 50, 0).get(0).id);
        verify(auth).requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER");
        verify(repository).billingSubscriptions("device", "active", "high", 50, 0);
    }

    @Test void summaryContainsLifecycleAndUsageMetrics() {
        AdminModels.BillingSubscriptionSummary summary = new AdminModels.BillingSubscriptionSummary(); summary.active = 8; summary.expiring = 2;
        when(repository.billingSubscriptionSummary()).thenReturn(summary);
        assertEquals(8, service.billingSubscriptionSummary(session).active); assertEquals(2, service.billingSubscriptionSummary(session).expiring);
    }

    @Test void invalidStatusAndPlanAreRejectedBeforeQuery() {
        assertEquals("invalid_status", assertThrows(AdminException.class, () -> service.billingSubscriptions(session, null, "cancelled", null, 50, 0)).code);
        assertEquals("invalid_plan", assertThrows(AdminException.class, () -> service.billingSubscriptions(session, null, null, "yearly", 50, 0)).code);
        verify(repository, never()).billingSubscriptions(any(), any(), any(), anyInt(), anyInt());
    }
}
