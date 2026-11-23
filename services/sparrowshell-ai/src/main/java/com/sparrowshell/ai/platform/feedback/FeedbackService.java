package com.sparrowshell.ai.platform.feedback;

import com.sparrowshell.ai.platform.account.AccountService;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Service;
import org.springframework.web.multipart.MultipartFile;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardCopyOption;
import java.time.Instant;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

@Service
public class FeedbackService {
    private static final long MAX_ATTACHMENT_BYTES = 10L * 1024L * 1024L;
    private static final Map<String, String> CONTENT_TYPES = contentTypes();
    private final FeedbackRepository repository;
    private final AccountService accounts;
    private final Path storageRoot;
    private final ConcurrentHashMap<String, RateWindow> rateWindows = new ConcurrentHashMap<String, RateWindow>();

    public FeedbackService(FeedbackRepository repository, AccountService accounts,
            @Value("${sparrowshell.feedback.storage-dir:data/feedback}") String storageDirectory) {
        this.repository = repository; this.accounts = accounts;
        this.storageRoot = Paths.get(storageDirectory).toAbsolutePath().normalize();
    }

    public FeedbackModels.Feedback submit(String authorization, String description, String contact,
            MultipartFile file, String remoteAddress) {
        String userId = null;
        if (authorization != null && !authorization.trim().isEmpty()) userId = accounts.authenticateAccountId(authorization);
        String cleanDescription = clean(description, 10_000);
        if (cleanDescription == null) throw new FeedbackException("invalid_description", "请填写问题描述，最多 10000 个字符");
        String cleanContact = clean(contact, 320);
        checkRateLimit(userId == null ? "anonymous:" + remoteAddress : "user:" + userId);
        FeedbackModels.Attachment attachment = file == null || file.isEmpty() ? null : attachment(file);
        FeedbackModels.Feedback feedback = new FeedbackModels.Feedback();
        feedback.id = UUID.randomUUID().toString(); feedback.userId = userId;
        feedback.description = cleanDescription; feedback.contact = cleanContact; feedback.status = "pending";
        feedback.createdAt = Instant.now(); feedback.updatedAt = feedback.createdAt; feedback.attachment = attachment;
        try {
            if (attachment != null) store(file, attachment.storageKey);
            repository.insert(feedback);
            return feedback;
        } catch (IOException exception) {
            if (attachment != null) deleteQuietly(attachment.storageKey);
            throw new FeedbackException("attachment_failed", "附件保存失败，请重试");
        } catch (RuntimeException exception) {
            if (attachment != null) deleteQuietly(attachment.storageKey);
            throw exception;
        }
    }

    public List<FeedbackModels.Feedback> list(String status, int limit, int offset) {
        if (status != null) validateStatus(status);
        if (limit < 1 || limit > 200 || offset < 0) throw new FeedbackException("invalid_pagination", "分页参数无效");
        return repository.list(status, limit, offset);
    }
    public long count(String status) { if (status != null) validateStatus(status); return repository.count(status); }
    public FeedbackModels.Feedback get(String id) {
        return repository.find(id).orElseThrow(() -> new FeedbackException("not_found", "反馈不存在"));
    }
    public FeedbackModels.Feedback updateStatus(String id, String status, String updatedBy) {
        validateStatus(status); return repository.updateStatus(id, status, updatedBy);
    }
    public Path attachmentPath(FeedbackModels.Attachment attachment) {
        if (attachment == null) throw new FeedbackException("not_found", "该反馈没有附件");
        Path path = storageRoot.resolve(attachment.storageKey).normalize();
        if (!path.startsWith(storageRoot) || !Files.isRegularFile(path)) throw new FeedbackException("not_found", "附件不存在或已移除");
        return path;
    }

    private FeedbackModels.Attachment attachment(MultipartFile file) {
        if (file.getSize() <= 0 || file.getSize() > MAX_ATTACHMENT_BYTES)
            throw new FeedbackException("invalid_attachment", "附件不能为空且不能超过 10 MB");
        String original = file.getOriginalFilename() == null ? "attachment" : file.getOriginalFilename().replace('\\', '/');
        original = original.substring(original.lastIndexOf('/') + 1).replaceAll("[\\p{Cntrl}]", "").trim();
        if (original.isEmpty()) original = "attachment";
        if (original.length() > 255) original = original.substring(original.length() - 255);
        String extension = "";
        int dot = original.lastIndexOf('.');
        if (dot >= 0) extension = original.substring(dot).toLowerCase(Locale.ROOT);
        String contentType = CONTENT_TYPES.get(extension);
        if (contentType == null) throw new FeedbackException("invalid_attachment", "仅支持图片、PDF、文本、日志或 ZIP 附件");
        FeedbackModels.Attachment attachment = new FeedbackModels.Attachment();
        attachment.originalName = original; attachment.storageKey = UUID.randomUUID().toString() + extension;
        attachment.contentType = contentType; attachment.sizeBytes = file.getSize(); return attachment;
    }

    private void store(MultipartFile file, String storageKey) throws IOException {
        Files.createDirectories(storageRoot);
        Path destination = storageRoot.resolve(storageKey).normalize();
        if (!destination.startsWith(storageRoot)) throw new FeedbackException("invalid_attachment", "附件存储路径无效");
        try (InputStream input = file.getInputStream()) { Files.copy(input, destination, StandardCopyOption.REPLACE_EXISTING); }
    }
    private void deleteQuietly(String storageKey) {
        try { Files.deleteIfExists(storageRoot.resolve(storageKey).normalize()); } catch (IOException ignored) { }
    }
    private void checkRateLimit(String key) {
        long now = System.currentTimeMillis();
        RateWindow window = rateWindows.computeIfAbsent(key, ignored -> new RateWindow(now));
        synchronized (window) {
            if (now - window.startedAt >= 3_600_000L) { window.startedAt = now; window.count = 0; }
            if (window.count >= 5) throw new FeedbackException("rate_limited", "提交过于频繁，请稍后再试");
            window.count++;
        }
    }
    private static String clean(String value, int maximum) {
        if (value == null) return null;
        String cleaned = value.trim();
        if (cleaned.isEmpty() || cleaned.length() > maximum) return null;
        return cleaned;
    }
    private static void validateStatus(String status) {
        if (!Arrays.asList("pending", "processing", "resolved").contains(status))
            throw new FeedbackException("invalid_status", "反馈处理状态无效");
    }
    private static Map<String, String> contentTypes() {
        Map<String, String> values = new HashMap<String, String>();
        values.put(".png", "image/png"); values.put(".jpg", "image/jpeg"); values.put(".jpeg", "image/jpeg");
        values.put(".gif", "image/gif"); values.put(".webp", "image/webp"); values.put(".pdf", "application/pdf");
        values.put(".txt", "text/plain"); values.put(".log", "text/plain"); values.put(".zip", "application/zip");
        return values;
    }
    private static final class RateWindow {
        long startedAt; int count;
        RateWindow(long startedAt) { this.startedAt = startedAt; }
    }
}
