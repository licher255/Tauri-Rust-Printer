// 日志管理，可以被多个组件使用

export type LogLevel = 'info' | 'success' | 'error' | 'warning';

export interface LogEntry {
  time: string;
  message: string;
  level: LogLevel;
}

class LogService {
  private logs: LogEntry[] = [];
  private listeners: ((logs: LogEntry[]) => void)[] = [];

  add(message: string, level: LogLevel = 'info') {
    const entry: LogEntry = {
      time: new Date().toLocaleTimeString(),
      message,
      level
    };
    this.logs.push(entry);
    
    // 限制日志数量，防止内存溢出
    if (this.logs.length > 100) {
      this.logs.shift();
    }
    
    this.notify();
  }

  getLogs(): LogEntry[] {
    return [...this.logs];
  }

  clear() {
    this.logs = [];
    this.notify();
  }

  onUpdate(callback: (logs: LogEntry[]) => void) {
    this.listeners.push(callback);
  }

  private notify() {
    this.listeners.forEach(cb => cb([...this.logs]));
  }
}

export const logService = new LogService();