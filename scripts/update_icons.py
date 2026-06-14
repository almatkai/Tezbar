import os
import shutil
import base64
import subprocess
from PIL import Image

def main():
    base_dir = "/Users/almatkairatov/Desktop/code/Raymes"
    src_icons_dir = os.path.join(base_dir, "AppIcons(3)")
    appiconset_dir = os.path.join(src_icons_dir, "Assets.xcassets", "AppIcon.appiconset")
    
    build_dir = os.path.join(base_dir, "build")
    resources_dir = os.path.join(base_dir, "resources")
    resources_icons_dir = os.path.join(resources_dir, "icons")
    
    # Ensure directories exist
    os.makedirs(build_dir, exist_ok=True)
    os.makedirs(resources_dir, exist_ok=True)
    os.makedirs(resources_icons_dir, exist_ok=True)
    
    # 1. Generate icon.icns using iconutil
    iconset_dir = os.path.join(build_dir, "icon.iconset")
    os.makedirs(iconset_dir, exist_ok=True)
    
    # Mapping according to standard iconutil naming conventions
    mapping = {
        "16.png": ["icon_16x16.png"],
        "32.png": ["icon_16x16@2x.png", "icon_32x32.png"],
        "64.png": ["icon_32x32@2x.png"],
        "128.png": ["icon_128x128.png"],
        "256.png": ["icon_128x128@2x.png", "icon_256x256.png"],
        "512.png": ["icon_256x256@2x.png", "icon_512x512.png"],
        "1024.png": ["icon_512x512@2x.png"]
    }
    
    for src_name, dest_names in mapping.items():
        src_path = os.path.join(appiconset_dir, src_name)
        if os.path.exists(src_path):
            for dest_name in dest_names:
                shutil.copyfile(src_path, os.path.join(iconset_dir, dest_name))
        else:
            print(f"Warning: {src_path} not found.")
            
    # Run iconutil
    icns_path = os.path.join(build_dir, "icon.icns")
    try:
        subprocess.run(["iconutil", "-c", "icns", iconset_dir, "-o", icns_path], check=True)
        print("Generated icon.icns successfully.")
    except Exception as e:
        print(f"Error running iconutil: {e}")
        
    # Clean up temporary iconset directory
    shutil.rmtree(iconset_dir)
    
    # 2. Generate icon.ico using Pillow
    ico_path = os.path.join(build_dir, "icon.ico")
    img_1024_path = os.path.join(appiconset_dir, "1024.png")
    if os.path.exists(img_1024_path):
        try:
            img = Image.open(img_1024_path)
            # Standard sizes for ICO
            sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
            img.save(ico_path, sizes=sizes)
            print("Generated icon.ico successfully.")
        except Exception as e:
            print(f"Error generating icon.ico: {e}")
            
    # 3. Copy/Generate icon.png (512x512)
    png_512_path = os.path.join(appiconset_dir, "512.png")
    shutil.copyfile(png_512_path, os.path.join(build_dir, "icon.png"))
    print("Copied icon.png (512x512) to build/icon.png.")
    
    # 4. Copy to resources/ and resources/icons/
    shutil.copyfile(icns_path, os.path.join(resources_dir, "icon.icns"))
    shutil.copyfile(ico_path, os.path.join(resources_dir, "icon.ico"))
    shutil.copyfile(os.path.join(build_dir, "icon.png"), os.path.join(resources_dir, "icon.png"))
    
    shutil.copyfile(icns_path, os.path.join(resources_icons_dir, "icon.icns"))
    shutil.copyfile(ico_path, os.path.join(resources_icons_dir, "icon.ico"))
    shutil.copyfile(os.path.join(build_dir, "icon.png"), os.path.join(resources_icons_dir, "icon.png"))
    print("Copied icons to resources/ and resources/icons/ successfully.")
    
    # 5. Base64 encode 1024.png to replace root SVGs
    if os.path.exists(img_1024_path):
        with open(img_1024_path, "rb") as image_file:
            encoded_string = base64.b64encode(image_file.read()).decode('utf-8')
            
        svg_content = f'<svg width="1024" height="1024" viewBox="0 0 1024 1024" fill="none" xmlns="http://www.w3.org/2000/svg">\n  <image href="data:image/png;base64,{encoded_string}" width="1024" height="1024" />\n</svg>\n'
        
        svgs = ["appIcon.svg", "appIcon_simple.svg", "appIcon_simple_blackwhite.svg"]
        for svg in svgs:
            with open(os.path.join(base_dir, svg), "w") as f:
                f.write(svg_content)
            print(f"Updated {svg} successfully.")

if __name__ == "__main__":
    main()
