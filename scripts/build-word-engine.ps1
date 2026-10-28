param([string]$Python = 'python', [string]$DistPath = 'tools')
$ErrorActionPreference = 'Stop'
$appRoot = Split-Path -Parent $PSScriptRoot
Push-Location $appRoot
try {
    $venv = Join-Path $appRoot '.tools/pdf-word-venv'
    if (-not (Test-Path "$venv/Scripts/python.exe")) {
        & $Python -m venv $venv
        if ($LASTEXITCODE) { throw 'Python virtual environment creation failed' }
    }
    $runtime = "$venv/Scripts/python.exe"
    & $runtime -m pip install -r engines/requirements.txt
    if ($LASTEXITCODE) { throw 'PDF engine dependencies failed' }
    $manifest = Get-Content engines/tessdata-manifest.json -Raw | ConvertFrom-Json
    New-Item engines/tessdata -ItemType Directory -Force | Out-Null
    foreach ($entry in $manifest.files) {
        $name = Split-Path $entry.Path -Leaf
        $target = Join-Path $appRoot "engines/tessdata/$name"
        if (-not (Test-Path -LiteralPath $target)) {
            Invoke-WebRequest "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/$($manifest.revision)/$name" -OutFile $target
        }
        if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne $entry.Hash) {
            throw "OCR data hash mismatch: $name"
        }
    }
    & $runtime -m PyInstaller --noconfirm --onedir --console --name pdf-word --distpath $DistPath --workpath .tools/pyinstaller-work --specpath .tools --add-data "$appRoot/engines/tessdata;tessdata" --collect-all pdf2docx --collect-all pymupdf --collect-data pptx --hidden-import PIL.AvifImagePlugin --exclude-module tkinter --exclude-module _tkinter --exclude-module onnxruntime --exclude-module pymupdf4llm --exclude-module pandas --exclude-module matplotlib --exclude-module torch engines/pdf_word.py
    if ($LASTEXITCODE) { throw 'Portable PDF engine build failed' }
} finally { Pop-Location }
