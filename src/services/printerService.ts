import { invoke } from "@tauri-apps/api/core";
import { logService } from "./logService";

// 类型定义
export interface Printer {
  name: string;
  id: string;
  status: "online" | "offline" | "busy"| string;
}

// 所有打印机相关的后端调用都在这里
export const printerApi = {
  // 获取列表
  async getList(): Promise<Printer[]> {
    logService.add("正在获取打印机列表...", "info");
    try {
        const printers = await invoke<Printer[]>("get_printers");
        
        // 调试输出
        console.log('后端返回的打印机:', JSON.stringify(printers, null, 2));
        
        logService.add(`找到 ${printers.length} 台打印机`, "success");
        return printers;
    } catch (error) {
        logService.add(`获取失败: ${error}`, "error");
        throw error;
    }
  },

  // 共享打印机
  async share(printerId: string): Promise<string> {
    console.log('前端传递的 printerId:', printerId);
    logService.add(`正在共享打印机: ${printerId}...`, "info");
    try {
      const result = await invoke<string>("share_printer", { printerId });
      logService.add(result, "success");
      return result;
    } catch (error) {
      logService.add(`共享失败: ${error}`, "error");
      throw error;
    }
  },

  // 停止共享
  async stop(printerId: string): Promise<void> {
    await invoke("stop_printer", { printerId });
    logService.add(`已停止共享: ${printerId}`, "info");
  },
  // 添加查询共享状态的方法
  async getSharedList(): Promise<Printer[]> {
      return await invoke<Printer[]>("get_shared_printers");
  },

  async unshare(printerId: string): Promise<void> {
      await invoke("unshare_printer", { printerId });
      logService.add(`已停止共享: ${printerId}`, "info");
  }
};

