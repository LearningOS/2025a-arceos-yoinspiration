#!/bin/bash
# 为 admin 用户配置 sudo 免密码
# 需要 root 权限执行此脚本

# 将 admin 用户添加到 sudo 组（如果还没有）
usermod -aG sudo admin 2>/dev/null

# 创建 sudoers.d 配置文件
echo "admin ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/admin
chmod 0440 /etc/sudoers.d/admin

# 验证配置
visudo -c

echo "配置完成！admin 用户现在可以无密码使用 sudo 命令。"

