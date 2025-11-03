# 配置 admin 用户 sudo 免密码

## 方法一：使用脚本（推荐）

1. 切换到 root 用户：
```bash
su -
```

2. 执行配置脚本：
```bash
bash /home/admin/open-camp-os/2025a-arceos-yoinspiration/setup_sudo_nopasswd.sh
```

3. 退出 root：
```bash
exit
```

## 方法二：手动配置

1. 切换到 root 用户：
```bash
su -
```

2. 创建 sudoers 配置：
```bash
echo "admin ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/admin
chmod 0440 /etc/sudoers.d/admin
```

3. 验证配置：
```bash
visudo -c
```

4. 退出 root：
```bash
exit
```

## 验证配置

执行以下命令，如果不需要输入密码就成功：
```bash
sudo -n true && echo "SUCCESS: Sudo works without password"
```

## 配置完成后的测试

配置完成后，运行以下命令执行完整测试：
```bash
cd /home/admin/open-camp-os/2025a-arceos-yoinspiration
export PATH="/opt/musl/x86_64-linux-musl-cross/bin:/opt/musl/aarch64-linux-musl-cross/bin:/opt/musl/riscv64-linux-musl-cross/bin:${PATH}"
./scripts/total-test.sh
```

预期结果：测试通过，得分 500/600 或更高。

