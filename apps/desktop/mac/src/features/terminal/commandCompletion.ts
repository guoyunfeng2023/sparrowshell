import type { CommandHistoryEntry } from '../../types';

export type CompletionSource = 'history' | 'common';

export interface CommandCompletion {
  command: string;
  source: CompletionSource;
  description: { zh: string; en: string } | null;
  highRisk: boolean;
}

type CommonCommand = Omit<CommandCompletion, 'source' | 'highRisk'> & { description: { zh: string; en: string } };

const commonCommands: CommonCommand[] = [
  { command: 'pwd', description: { zh: '显示当前目录', en: 'Print the working directory' } },
  { command: 'ls -lah', description: { zh: '查看目录内容和隐藏文件', en: 'List files, including hidden files' } },
  { command: 'df -h', description: { zh: '查看磁盘空间', en: 'Show disk usage' } },
  { command: 'du -sh *', description: { zh: '统计当前目录各项大小', en: 'Summarize sizes in this directory' } },
  { command: 'free -h', description: { zh: '查看内存使用情况', en: 'Show memory usage' } },
  { command: 'uptime', description: { zh: '查看运行时间和负载', en: 'Show uptime and load averages' } },
  { command: 'uname -a', description: { zh: '查看内核和系统架构', en: 'Show kernel and architecture' } },
  { command: 'cat /etc/os-release', description: { zh: '查看操作系统版本', en: 'Show the operating system release' } },
  { command: 'ps aux --sort=-%cpu', description: { zh: '按 CPU 占用查看进程', en: 'List processes sorted by CPU' } },
  { command: 'ps aux --sort=-%mem', description: { zh: '按内存占用查看进程', en: 'List processes sorted by memory' } },
  { command: 'top', description: { zh: '实时查看进程和资源', en: 'Monitor processes and resources' } },
  { command: 'journalctl -xe', description: { zh: '查看最近的系统日志', en: 'Inspect recent system logs' } },
  { command: 'journalctl -f', description: { zh: '持续跟踪系统日志', en: 'Follow system logs' } },
  { command: 'journalctl -u nginx', description: { zh: '查看 Nginx 服务日志', en: 'Show Nginx service logs' } },
  { command: 'journalctl -u docker', description: { zh: '查看 Docker 服务日志', en: 'Show Docker service logs' } },
  { command: 'systemctl status', description: { zh: '查看系统或服务状态', en: 'Show system or service status' } },
  { command: 'systemctl status nginx', description: { zh: '查看 Nginx 服务状态', en: 'Show Nginx service status' } },
  { command: 'systemctl status docker', description: { zh: '查看 Docker 服务状态', en: 'Show Docker service status' } },
  { command: 'systemctl start', description: { zh: '启动服务（需补充服务名）', en: 'Start a service (add a service name)' } },
  { command: 'systemctl restart', description: { zh: '重启服务（需补充服务名）', en: 'Restart a service (add a service name)' } },
  { command: 'systemctl reload', description: { zh: '重载服务配置（需补充服务名）', en: 'Reload a service (add a service name)' } },
  { command: 'systemctl stop', description: { zh: '停止服务（需补充服务名）', en: 'Stop a service (add a service name)' } },
  { command: 'systemctl enable', description: { zh: '设置开机启动（需补充服务名）', en: 'Enable at boot (add a service name)' } },
  { command: 'systemctl list-units --type=service', description: { zh: '列出 Systemd 服务', en: 'List Systemd services' } },
  { command: 'docker ps', description: { zh: '查看运行中的容器', en: 'List running containers' } },
  { command: 'docker ps -a', description: { zh: '查看全部容器', en: 'List all containers' } },
  { command: 'docker images', description: { zh: '查看本地镜像', en: 'List local images' } },
  { command: 'docker logs --tail 100', description: { zh: '查看容器最近日志（需补充容器名）', en: 'Show recent logs (add a container name)' } },
  { command: 'docker stats', description: { zh: '查看容器资源占用', en: 'Show container resource usage' } },
  { command: 'docker inspect', description: { zh: '查看对象详情（需补充名称或 ID）', en: 'Inspect an object (add a name or ID)' } },
  { command: 'docker compose ps', description: { zh: '查看 Compose 服务状态', en: 'List Compose services' } },
  { command: 'docker compose logs -f', description: { zh: '持续查看 Compose 日志', en: 'Follow Compose service logs' } },
  { command: 'docker compose config', description: { zh: '检查 Compose 配置', en: 'Validate and render Compose config' } },
  { command: 'docker compose up -d', description: { zh: '后台启动 Compose 服务', en: 'Start Compose services in background' } },
  { command: 'docker compose down', description: { zh: '停止并移除 Compose 资源', en: 'Stop and remove Compose resources' } },
  { command: 'git status', description: { zh: '查看工作区状态', en: 'Show working tree status' } },
  { command: 'git pull', description: { zh: '拉取并整合远端修改', en: 'Fetch and integrate remote changes' } },
  { command: 'git log --oneline -10', description: { zh: '查看最近十条提交', en: 'Show the latest ten commits' } },
  { command: 'tail -f', description: { zh: '持续跟踪文件（需补充文件路径）', en: 'Follow a file (add a file path)' } },
  { command: 'tail -n 100', description: { zh: '查看文件最后 100 行（需补充文件路径）', en: 'Show the last 100 lines (add a file path)' } },
  { command: 'grep -r', description: { zh: '递归搜索（需补充关键词和路径）', en: 'Search recursively (add a pattern and path)' } },
  { command: 'find . -name', description: { zh: '按名称查找（需补充名称）', en: 'Find by name (add a name pattern)' } },
  { command: 'tar -tzf', description: { zh: '查看 tar.gz 内容（需补充压缩包路径）', en: 'List tar.gz contents (add an archive path)' } },
  { command: 'tar -xzf', description: { zh: '解压 tar.gz（需补充压缩包路径）', en: 'Extract tar.gz (add an archive path)' } },
  { command: 'tar -czf', description: { zh: '创建 tar.gz（需补充压缩包和源路径）', en: 'Create tar.gz (add archive and source paths)' } },
  { command: 'zip -r', description: { zh: '创建 ZIP（需补充压缩包和源路径）', en: 'Create ZIP (add archive and source paths)' } },
  { command: 'unzip', description: { zh: '解压 ZIP（需补充压缩包路径）', en: 'Extract ZIP (add an archive path)' } },
  { command: 'unzip -l', description: { zh: '查看 ZIP 内容（需补充压缩包路径）', en: 'List ZIP contents (add an archive path)' } },
  { command: 'nginx -t', description: { zh: '检查 Nginx 配置', en: 'Test Nginx configuration' } },
  { command: 'ss -lntp', description: { zh: '查看 TCP 监听端口', en: 'Show listening TCP ports' } },
  { command: 'curl -I', description: { zh: '查看 HTTP 响应头（需补充 URL）', en: 'Fetch HTTP headers (add a URL)' } },
];

const sensitivePattern = /(?:password|passwd|passphrase|secret|token|api[_-]?key|access[_-]?key|authorization|private[_-]?key)\s*(?:=|:|\s)/i;
const highRiskPattern = /(?:^|\s)(?:rm\s+-\S*(?:rf|fr)\S*|mkfs(?:\.\S+)?\s|dd\s+.*\bof=|shutdown\b|reboot\b|docker\s+compose\s+down\b|systemctl\s+(?:stop|disable)\b)/i;

export function isHighRiskCompletion(command: string): boolean {
  return highRiskPattern.test(command);
}

function isSensitive(command: string): boolean {
  return sensitivePattern.test(command);
}

/**
 * Returns suffix-only completions. Restricting matches to the exact current
 * prefix lets the terminal append text without clearing or executing a line.
 */
export function getCommandCompletions(input: string, history: CommandHistoryEntry[], limit = 8): CommandCompletion[] {
  if (input.length > 500 || /[\r\n\0]/.test(input) || input !== input.trimStart()) return [];
  const seen = new Set<string>();
  const matches: CommandCompletion[] = [];

  for (const entry of history) {
    const command = entry.command.trim();
    if (!command.startsWith(input) || command === input || command.length > 500 || isSensitive(command) || seen.has(command)) continue;
    seen.add(command);
    matches.push({ command, source: 'history', description: commonCommands.find((item) => item.command === command)?.description ?? null, highRisk: isHighRiskCompletion(command) });
    if (matches.length >= limit) return matches;
  }

  for (const item of commonCommands) {
    if (!item.command.startsWith(input) || item.command === input || seen.has(item.command)) continue;
    seen.add(item.command);
    matches.push({ ...item, source: 'common', highRisk: isHighRiskCompletion(item.command) });
    if (matches.length >= limit) break;
  }
  return matches;
}
