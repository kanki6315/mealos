#!/usr/bin/env python3
"""Generate the checked-in Xcode project using only Python's standard library.
Rerun after adding sources or resources.
"""
import hashlib, json, pathlib, plistlib
root = pathlib.Path(__file__).resolve().parents[1]
objects = {}
def ident(key): return hashlib.sha256(key.encode()).hexdigest()[:24].upper()
def add(key, isa, **fields):
    key = ident(key); objects[key] = dict(isa=isa, **fields); return key

def ref(path, kind): return add('file:' + path, 'PBXFileReference', lastKnownFileType=kind, path=path, sourceTree='<group>')
sources = sorted(root.glob('MealOS/**/*.swift'))
resources = sorted(p for p in (root / 'MealOS/Resources').iterdir() if p.is_file() or p.suffix in ('.xcassets', '.icon'))
source_refs = [ref(str(p.relative_to(root)), 'sourcecode.swift') for p in sources]
resource_refs = [ref(str(p.relative_to(root)), 'folder.iconcomposer' if p.suffix == '.icon' else 'folder.assetcatalog' if p.suffix == '.xcassets' else 'text.plist.xml' if p.suffix == '.plist' else 'text.json' if p.suffix == '.json' else 'file') for p in resources]
src_phase = add('sources', 'PBXSourcesBuildPhase', buildActionMask=2147483647, files=[add('build:'+r,'PBXBuildFile',fileRef=r) for r in source_refs], runOnlyForDeploymentPostprocessing=0)
res_phase = add('resources', 'PBXResourcesBuildPhase', buildActionMask=2147483647, files=[add('build:'+r,'PBXBuildFile',fileRef=r) for r in resource_refs], runOnlyForDeploymentPostprocessing=0)
packages = []; products = []
frameworks = add('frameworks','PBXFrameworksBuildPhase',buildActionMask=2147483647,files=[add('link:'+r,'PBXBuildFile',productRef=r) for r in products],runOnlyForDeploymentPostprocessing=0)
product = add('app','PBXFileReference',explicitFileType='wrapper.application',includeInIndex=0,path='Forkware.app',sourceTree='BUILT_PRODUCTS_DIR')
product_group = add('products','PBXGroup',children=[product],name='Products',sourceTree='<group>')
config_ref = ref('Config.xcconfig','text.xcconfig')
info_ref = ref('MealOS/Info.plist','text.plist.xml')
group = add('root','PBXGroup',children=[add('sourceGroup','PBXGroup',children=source_refs,name='Source',sourceTree='<group>'),add('resourceGroup','PBXGroup',children=resource_refs,name='Resources',sourceTree='<group>'),config_ref,info_ref,product_group],sourceTree='<group>')
common = {'SDKROOT':'iphoneos','IPHONEOS_DEPLOYMENT_TARGET':'17.0','SWIFT_VERSION':'5.0','CLANG_ENABLE_MODULES':'YES','CLANG_ENABLE_OBJC_ARC':'YES','ENABLE_USER_SCRIPT_SANDBOXING':'YES'}
app_settings = {'PRODUCT_BUNDLE_IDENTIFIER':'com.arjunakankipati.forkware','PRODUCT_NAME':'$(TARGET_NAME)','TARGETED_DEVICE_FAMILY':'1,2','INFOPLIST_FILE':'MealOS/Info.plist','GENERATE_INFOPLIST_FILE':'NO','CODE_SIGN_STYLE':'Automatic','DEVELOPMENT_TEAM':'TDBAHN96B3','CURRENT_PROJECT_VERSION':'6','MARKETING_VERSION':'0.3.1','CODE_SIGN_ENTITLEMENTS':'$(FORKWARE_ENTITLEMENTS)','SUPPORTED_PLATFORMS':'iphoneos iphonesimulator','SUPPORTS_MACCATALYST':'NO','ASSETCATALOG_COMPILER_APPICON_NAME':'AppIcon','LD_RUNPATH_SEARCH_PATHS':['$(inherited)','@executable_path/Frameworks'],'SWIFT_EMIT_LOC_STRINGS':'YES'}
def configs(prefix, values):
    items=[]
    for name in ['Debug','Release']:
        settings=dict(values)
        settings.update(SWIFT_OPTIMIZATION_LEVEL='-Onone' if name == 'Debug' else '-O',DEBUG_INFORMATION_FORMAT='dwarf' if name == 'Debug' else 'dwarf-with-dsym')
        if name == 'Debug':
            settings['SWIFT_ACTIVE_COMPILATION_CONDITIONS']='DEBUG'
            settings['ONLY_ACTIVE_ARCH']='YES'
        items.append(add(prefix+name,'XCBuildConfiguration',name=name,buildSettings=settings,**({'baseConfigurationReference':config_ref} if prefix=='target' else {})))
    return add(prefix+'configs','XCConfigurationList',buildConfigurations=items,defaultConfigurationIsVisible=0,defaultConfigurationName='Release')
target = add('target','PBXNativeTarget',buildConfigurationList=configs('target',app_settings),buildPhases=[src_phase,frameworks,res_phase],buildRules=[],dependencies=[],name='Forkware',packageProductDependencies=products,productName='Forkware',productReference=product,productType='com.apple.product-type.application')
ui_source = ref('UITests/MealOSUITests.swift', 'sourcecode.swift')
ui_product = add('ui-product', 'PBXFileReference', explicitFileType='wrapper.cfbundle', includeInIndex=0, path='ForkwareUITests.xctest', sourceTree='BUILT_PRODUCTS_DIR')
objects[group]['children'].append(ui_source)
objects[product_group]['children'].append(ui_product)
ui_sources = add('ui-sources', 'PBXSourcesBuildPhase', buildActionMask=2147483647, files=[add('ui-build', 'PBXBuildFile', fileRef=ui_source)], runOnlyForDeploymentPostprocessing=0)
proxy = add('ui-proxy', 'PBXContainerItemProxy', containerPortal=ident('project'), proxyType=1, remoteGlobalIDString=target, remoteInfo='Forkware')
dependency = add('ui-dependency', 'PBXTargetDependency', target=target, targetProxy=proxy)
ui_settings = {'PRODUCT_BUNDLE_IDENTIFIER':'com.arjunakankipati.forkware.uitests','PRODUCT_NAME':'$(TARGET_NAME)','GENERATE_INFOPLIST_FILE':'YES','TEST_TARGET_NAME':'Forkware','CODE_SIGN_STYLE':'Automatic','TARGETED_DEVICE_FAMILY':'1,2','LD_RUNPATH_SEARCH_PATHS':['$(inherited)','@executable_path/Frameworks','@loader_path/Frameworks']}
ui_target = add('ui-target', 'PBXNativeTarget', buildConfigurationList=configs('uitest', ui_settings), buildPhases=[ui_sources], buildRules=[], dependencies=[dependency], name='ForkwareUITests', productName='ForkwareUITests', productReference=ui_product, productType='com.apple.product-type.bundle.ui-testing')
project = add('project' ,'PBXProject',attributes={'BuildIndependentTargetsInParallel':'YES','LastSwiftUpdateCheck':'1600','LastUpgradeCheck':'1600'},buildConfigurationList=configs('project',common),compatibilityVersion='Xcode 14.0',developmentRegion='en',hasScannedForEncodings=0,knownRegions=['en','Base'],mainGroup=group,packageReferences=packages,productRefGroup=product_group,projectDirPath='',projectRoot='',targets=[target,ui_target])
def format_value(v, level=0):
    if isinstance(v,dict): return '{\n' + ''.join('\t'*(level+1)+json.dumps(str(k))+' = '+format_value(value,level+1)+';\n' for k,value in v.items())+'\t'*level+'}'
    if isinstance(v,list): return '(' + ', '.join(format_value(x,level) for x in v) + (',' if v else '') + ')'
    if isinstance(v,int): return str(v)
    return json.dumps(v)
folder=root/'Forkware.xcodeproj'; folder.mkdir(exist_ok=True)
(folder/'project.pbxproj').write_text('// !$*UTF8*$!\n'+format_value({'archiveVersion':1,'classes':{},'objectVersion':56,'objects':objects,'rootObject':project})+'\n')
scheme=folder/'xcshareddata/xcschemes'; scheme.mkdir(parents=True,exist_ok=True)
ui_reference=f'<BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="{ui_target}" BuildableName="ForkwareUITests.xctest" BlueprintName="ForkwareUITests" ReferencedContainer="container:Forkware.xcodeproj"/>'
reference=f'<BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="{target}" BuildableName="Forkware.app" BlueprintName="Forkware" ReferencedContainer="container:Forkware.xcodeproj"/>'
(scheme/'Forkware.xcscheme').write_text(f'''<?xml version="1.0" encoding="UTF-8"?>
<Scheme LastUpgradeVersion="1600" version="1.3">
<BuildAction parallelizeBuildables="YES" buildImplicitDependencies="YES"><BuildActionEntries><BuildActionEntry buildForTesting="YES" buildForRunning="YES" buildForProfiling="YES" buildForArchiving="YES" buildForAnalyzing="YES">{reference}</BuildActionEntry></BuildActionEntries></BuildAction>
<TestAction buildConfiguration="Debug" shouldUseLaunchSchemeArgsEnv="YES"><Testables><TestableReference skipped="NO">{ui_reference}</TestableReference></Testables></TestAction>
<LaunchAction buildConfiguration="Debug" selectedDebuggerIdentifier="Xcode.DebuggerFoundation.Debugger.LLDB" selectedLauncherIdentifier="Xcode.IDEFoundation.Launcher.LLDB" launchStyle="0" useCustomWorkingDirectory="NO" ignoresPersistentStateOnLaunch="NO" debugDocumentVersioning="YES" debugServiceExtension="internal" allowLocationSimulation="YES"><BuildableProductRunnable runnableDebuggingMode="0">{reference}</BuildableProductRunnable></LaunchAction>
<ProfileAction buildConfiguration="Release" shouldUseLaunchSchemeArgsEnv="YES" useCustomWorkingDirectory="NO" debugDocumentVersioning="YES"><BuildableProductRunnable runnableDebuggingMode="0">{reference}</BuildableProductRunnable></ProfileAction>
<AnalyzeAction buildConfiguration="Debug"/><ArchiveAction buildConfiguration="Release" revealArchiveInOrganizer="YES"/>
</Scheme>\n''')
print(f'Generated {folder} ({len(sources)} Swift files, {len(resources)} resources)')
