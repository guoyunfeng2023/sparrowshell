package com.sparrowshell.ai.platform.admin;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.transaction.PlatformTransactionManager;
import java.util.Collections;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

class AdminBillingServiceTest {
    private final AdminRepository repository = mock(AdminRepository.class);
    private final AdminAuthService auth = mock(AdminAuthService.class);
    private final ObjectProvider<PlatformTransactionManager> provider = mock(ObjectProvider.class);
    private final AdminModels.AdminSession session = new AdminModels.AdminSession();
    private AdminService service;

    @BeforeEach void setUp() {
        when(provider.getIfAvailable()).thenReturn(null);
        service = new AdminService(repository, auth, provider);
        session.adminId = "admin-1"; session.email = "admin@example.com"; session.role = "SUPER_ADMIN";
    }

    @Test void ordersUseReadRoleAndValidatedFilters() {
        AdminModels.BillingOrder order = new AdminModels.BillingOrder(); order.id = "order-1"; order.status = "paid";
        when(repository.billingOrders("order", "paid", 50, 0)).thenReturn(Collections.singletonList(order));
        assertEquals("order-1", service.billingOrders(session, "order", "paid", 50, 0).get(0).id);
        verify(auth).requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER");
        verify(repository).billingOrders("order", "paid", 50, 0);
    }

    @Test void licensesUseReadRoleAndReturnRealRows() {
        AdminModels.BillingLicense license = new AdminModels.BillingLicense(); license.id = "license-1"; license.boundDeviceId = "device-1";
        when(repository.billingLicenses("device", "bound", 50, 0)).thenReturn(Collections.singletonList(license));
        assertEquals("device-1", service.billingLicenses(session, "device", "bound", 50, 0).get(0).boundDeviceId);
        verify(auth).requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER");
    }

    @Test void completeLicenseKeyUsesOnlyItsHashForExactLookup() {
        String key = "ss-1111-2222-3333-4444-5555-6666-7777-8888";
        String normalized = key.toUpperCase();
        String hash = com.sparrowshell.ai.platform.billing.AiTaskQuotaService.hash(normalized);
        AdminModels.BillingLicense license = new AdminModels.BillingLicense(); license.id = "license-1";
        when(repository.billingLicensesByKeyHash(hash, "bound", 50, 0)).thenReturn(Collections.singletonList(license));
        when(repository.billingLicenseCountByKeyHash(hash, "bound")).thenReturn(1L);

        assertEquals("license-1", service.billingLicenses(session, key, "bound", 50, 0).get(0).id);
        assertEquals(1L, service.billingLicenseCount(session, key, "bound"));
        verify(repository).billingLicensesByKeyHash(hash, "bound", 50, 0);
        verify(repository).billingLicenseCountByKeyHash(hash, "bound");
        verify(repository, never()).billingLicenses(any(), any(), anyInt(), anyInt());
        verify(repository, never()).billingLicenseCount(any(), any());
    }

    @Test void invalidOrderStatusIsRejectedBeforeQuery() {
        AdminException error = assertThrows(AdminException.class, () -> service.billingOrders(session, null, "refunded", 50, 0));
        assertEquals("invalid_status", error.code);
        verify(repository, never()).billingOrders(any(), any(), anyInt(), anyInt());
    }

    @Test void invalidLicenseStatusIsRejectedBeforeQuery() {
        AdminException error = assertThrows(AdminException.class, () -> service.billingLicenses(session, null, "active", 50, 0));
        assertEquals("invalid_status", error.code);
        verify(repository, never()).billingLicenses(any(), any(), anyInt(), anyInt());
    }
}
