# modpacks-rs  
Downloads curseforge or FTB modpacks  
# Usage:  
Before any other arguments, if you wish to use multithreaded downloads to speed up the process pass `--threads n` where n is the number of threads you want  

Obtain a pack id from curseforge/FTB or use one of the search features (more info in the `modpacks-rs help` command), then run `modpacks-rs (ftb or cf) download id version`, where version is either a version ID or `latest`, to get the latest version

# Caveats
Curseforge server installations will not work if the modpack client files contain client only mods, if you know how to download additional files from curseforge without requiring the user to manually get both a pack text id and a file id, any help is appreciated
