# AI-OS Template Installer
# استخدام: powershell -ExecutionPolicy Bypass -File install.ps1
# يسأل: مسار المشروع + اسم المشروع → ينسخ القالب ويستبدل الاسم تلقائيًا

param(
    [string]$Target,
    [string]$ProjectName
)

$ErrorActionPreference = "Stop"

Write-Host ""
Write-Host "==============================================" -ForegroundColor Cyan
Write-Host "  AI-OS Template Installer" -ForegroundColor Cyan
Write-Host "==============================================" -ForegroundColor Cyan

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

if (-not $Target) {
    $Target = Read-Host "المسار الكامل للمشروع الجديد (مثلا D:\MyProject)"
}
$Target = $Target.Trim('"').TrimEnd('\')

if (-not (Test-Path -LiteralPath $Target)) {
    Write-Host "المسار مش موجود — بنسحبه لوحده..." -ForegroundColor Yellow
    New-Item -ItemType Directory -Path $Target -Force | Out-Null
}

if (-not $ProjectName) {
    $default = Split-Path $Target -Leaf
    $ProjectName = Read-Host "اسم المشروع (يظهر في القالب) [افتراضي: $default]"
    if (-not $ProjectName) { $ProjectName = $default }
}

$placeholders = "AI_MANDATE.md","AGENTS.md","RESEARCH_PROTOCOL.md","RESEARCH_LOG.md","EXECUTION_AGENDA.md","CODE_MAP.md","CODE_PATTERNS.md","TEST_RUNNER.md","SECURITY.md","README.md","INSTALL.md","CHANGELOG.md"

$utf8 = New-Object System.Text.UTF8Encoding($false)
$copied = 0
$skipped = 0

foreach ($fn in $placeholders) {
    $src = Join-Path $scriptDir $fn
    $dst = Join-Path $Target $fn
    if (-not (Test-Path -LiteralPath $src)) { continue }

    if (Test-Path -LiteralPath $dst) {
        # لو الملف موجود ومعدّل (مفيش placeholder) — ما تكتبش فوقه
        $existing = [System.IO.File]::ReadAllText($dst, [System.Text.Encoding]::UTF8)
        if (-not $existing.Contains("{{PROJECT_NAME}}")) {
            Write-Host "  SKIP (موجود مخصص): $fn" -ForegroundColor Yellow
            $skipped++
            continue
        }
    }

    $content = [System.IO.File]::ReadAllText($src, [System.Text.Encoding]::UTF8)
    $content = $content.Replace("{{PROJECT_NAME}}", $ProjectName)
    $content = $content.Replace("{{INSTALL_DATE}}", (Get-Date -Format "d MMMM yyyy"))
    [System.IO.File]::WriteAllText($dst, $content, $utf8)
    $copied++
}

Write-Host ""
Write-Host "==============================================" -ForegroundColor Green
Write-Host "  تم التركيب في: $Target" -ForegroundColor Green
Write-Host "  ملفات منسوخة: $copied  |  ملتفى عليها (مخصصة): $skipped" -ForegroundColor Green
Write-Host "==============================================" -ForegroundColor Green
Write-Host ""

Write-Host "التعديل اليدوي المطلوب (بعد التركيب):" -ForegroundColor Yellow
Write-Host "  1. EXECUTION_AGENDA.md  → مهام مشروعك الحقيقية"
Write-Host "  2. CODE_MAP.md          → خريطة ملفات مشروعك"
Write-Host "  3. CODE_PATTERNS.md     → أنماط من كودك الفعلي"
Write-Host "  4. TEST_RUNNER.md       → أوامر build/test بتاعتك"
Write-Host ""
Write-Host "الجلسة الجاية: افتح AI على المشروع → هيقرا AGENTS.md تلقائيًا → يطبع البيعة."
Write-Host ""
