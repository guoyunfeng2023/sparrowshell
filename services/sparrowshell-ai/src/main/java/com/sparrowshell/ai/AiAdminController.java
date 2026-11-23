package com.sparrowshell.ai;

import org.springframework.beans.factory.ObjectProvider;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.server.ResponseStatusException;
import com.sparrowshell.ai.platform.admin.AdminAccessGuard;
import javax.servlet.http.HttpServletRequest;
import java.math.BigDecimal;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.*;

/** Local read-only operational metadata. No prompt, command, target or credential fields. */
@RestController
@RequestMapping("/v1/admin/ai")
public class AiAdminController {
    private final AiUsageLedgerRepository repository;
    private final AdminAccessGuard accessGuard;

    @Autowired
    AiAdminController(ObjectProvider<AiUsageLedgerRepository> repository, AdminAccessGuard accessGuard) {
        this.repository = repository.getIfAvailable(); this.accessGuard = accessGuard;
    }

    AiAdminController(AiUsageLedgerRepository repository, AdminAccessGuard accessGuard) {
        this.repository = repository; this.accessGuard = accessGuard;
    }

    @GetMapping("/overview")
    Map<String, Object> overview(HttpServletRequest request, @RequestParam(defaultValue = "today") String range) {
        accessGuard.requireAiRead(request);
        long days;
        if ("today".equals(range)) days = 1;
        else if ("7d".equals(range)) days = 7;
        else if ("30d".equals(range)) days = 30;
        else throw new ResponseStatusException(HttpStatus.BAD_REQUEST, "unsupported range");
        Instant to = Instant.now();
        Instant from = to.minus(days, ChronoUnit.DAYS);
        Map<String, Object> response = new LinkedHashMap<String, Object>();
        response.put("range", range);
        response.put("from", from.toString());
        response.put("to", to.toString());
        response.put("generatedAt", to.toString());
        response.put("users", unavailable("account_service_not_connected"));
        response.put("sync", unavailable("sync_service_not_connected"));
        response.put("ai", repository == null ? unavailable("usage_ledger_not_configured") : aggregate(from, to));
        response.put("status", ((Map<?, ?>) response.get("ai")).get("status"));
        return response;
    }

    private Map<String, Object> aggregate(Instant from, Instant to) {
        try {
            List<AiUsage> rows = repository.between(from, to);
            long succeeded = 0, failed = 0, reserved = 0, unknown = 0;
            long input = 0, output = 0;
            BigDecimal cost = BigDecimal.ZERO;
            Map<String, List<AiUsage>> routes = new LinkedHashMap<String, List<AiUsage>>();
            for (AiUsage row : rows) {
                if ("succeeded".equals(row.status)) succeeded++;
                else if ("failed".equals(row.status)) failed++;
                else if ("reserved".equals(row.status)) reserved++;
                if (!"reported".equals(row.usageStatus)) unknown++;
                if (row.inputTokens != null) input += row.inputTokens;
                if (row.outputTokens != null) output += row.outputTokens;
                if (row.actualCost != null) cost = cost.add(row.actualCost);
                routes.computeIfAbsent(row.routeId, ignored -> new ArrayList<AiUsage>()).add(row);
            }
            Map<String, Object> result = new LinkedHashMap<String, Object>();
            result.put("status", "available");
            result.put("requests", rows.size());
            result.put("succeeded", succeeded);
            result.put("failed", failed);
            result.put("reserved", reserved);
            result.put("inputTokens", input);
            result.put("outputTokens", output);
            result.put("tokenStatus", unknown == 0 ? "complete" : "partial");
            result.put("unknownUsage", unknown);
            result.put("estimatedCost", cost);
            result.put("costStatus", unknown == 0 ? "complete" : "partial");
            result.put("priceVersions", priceVersions(rows));
            List<Map<String, Object>> routeRows = new ArrayList<Map<String, Object>>();
            for (Map.Entry<String, List<AiUsage>> entry : routes.entrySet())
                routeRows.add(route(entry.getKey(), entry.getValue()));
            result.put("routes", routeRows);
            result.put("recent", recent(rows));
            return result;
        } catch (RuntimeException exception) {
            return unavailable("usage_ledger_query_failed");
        }
    }

    private Map<String, Object> route(String routeId, List<AiUsage> rows) {
        long succeeded = rows.stream().filter(row -> "succeeded".equals(row.status)).count();
        long unknown = rows.stream().filter(row -> !"reported".equals(row.usageStatus)).count();
        BigDecimal cost = rows.stream().map(row -> row.actualCost).filter(Objects::nonNull)
                .reduce(BigDecimal.ZERO, BigDecimal::add);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("routeId", routeId);
        result.put("requests", rows.size());
        result.put("succeeded", succeeded);
        result.put("failed", rows.stream().filter(row -> "failed".equals(row.status)).count());
        result.put("estimatedCost", cost);
        result.put("costStatus", unknown == 0 ? "complete" : "partial");
        result.put("priceVersions", priceVersions(rows));
        return result;
    }

    private List<Map<String, Object>> recent(List<AiUsage> rows) {
        List<AiUsage> sorted = new ArrayList<AiUsage>(rows);
        sorted.sort(Comparator.comparing(row -> row.createdAt, Comparator.nullsLast(Comparator.reverseOrder())));
        List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();
        for (AiUsage row : sorted.subList(0, Math.min(50, sorted.size()))) {
            Map<String, Object> item = new LinkedHashMap<String, Object>();
            item.put("clientRequestId", row.clientRequestId);
            item.put("routeId", row.routeId);
            item.put("operation", row.operation);
            item.put("status", row.status);
            item.put("inputTokens", row.inputTokens);
            item.put("outputTokens", row.outputTokens);
            item.put("estimatedCost", row.actualCost);
            item.put("usageStatus", row.usageStatus);
            item.put("priceVersion", row.priceVersion);
            item.put("createdAt", row.createdAt == null ? null : row.createdAt.toString());
            result.add(item);
        }
        return result;
    }

    private List<String> priceVersions(List<AiUsage> rows) {
        Set<String> versions = new TreeSet<String>();
        for (AiUsage row : rows) if (row.priceVersion != null) versions.add(row.priceVersion);
        return new ArrayList<String>(versions);
    }

    private Map<String, Object> unavailable(String reason) {
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("status", "unavailable");
        result.put("reason", reason);
        return result;
    }
}
