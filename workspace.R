#install_github('saeyslab/CytoNorm')

library(tercen)
library(dplyr)
library(flowCore)
library(FlowSOM)
library(devtools)
library(CytoNorm)
library(tidyr)
#system.file("extdata", package = "CytoNorm")


#docID
options("tercen.workflowId" = "8e1125259c02a2b6da8de281bd00c4ed")
options("tercen.stepId"     = "ffdacc8e-a85d-4752-86bc-b7c23b17df8a")

getOption("tercen.workflowId")
getOption("tercen.stepId")

############################### FUNCTION
# fcs_to_data
# input filename of fcs data
# output dataframe of the fcs data
fcs_to_data = function(filename) {
  data_fcs = read.FCS(filename, transformation = FALSE)
  names_parameters = data_fcs@parameters@data$desc
  data = as.data.frame(exprs(data_fcs))
  col_names = colnames(data)
  # Column added late can have "<NA>" as a description without been detected as NA
  names_parameters = ifelse(names_parameters == "<NA>",NA,names_parameters)
  names_parameters = ifelse(is.na(names_parameters),col_names,names_parameters)
  colnames(data) = names_parameters
  data %>%
    mutate_if(is.logical, as.character) %>%
    mutate_if(is.integer, as.double) %>%
    mutate(.ci = as.integer(rep_len(0, nrow(.)))) %>%
    mutate(filename = rep_len(basename(filename), nrow(.)))
}

CytoNorm.normalize.custom <- function(model,
                                      files,
                                      labels,
                                      channels,
                                      transformList,
                                      transformList.reverse,
                                      outputDir = ".",
                                      prefix = "Norm_",
                                      clean = TRUE,
                                      verbose = FALSE,
                                      normMethod.normalize = QuantileNorm.normalize){
  if(is.null(model$fsom) |
     is.null(model$clusterRes)){
    stop("The 'model' paramter should be the result of using the
             trainQuantiles function.")
  }
  
  if(length(labels) != length(files)){
    stop("Input parameters 'labels' and 'files' should have the same length")
  }
  
  # Create output directory
  if(!dir.exists(outputDir)){
    dir.create(outputDir)
  }
  
  fsom <- model$fsom
  clusterRes <- model$clusterRes
  
  # Split files by clusters
  cellClusterIDs <- list()
  meta <- list()
  cluster_files <- list()
  for(file in files){
    if(verbose) message("Splitting ",file)
    ff <- flowCore::read.FCS(file)
    
    if(!is.null(transformList)){
      ff <- flowCore::transform(ff, transformList)
      # meta[[file]] <- list()
      # meta[[file]][["description_original"]] <- ff@description
      # meta[[file]][["parameters_original"]] <- ff@parameters
    }
    
    fsom_file <- FlowSOM::NewData(fsom,ff)
    
    cellClusterIDs[[file]] <- FlowSOM::GetMetaclusters(fsom_file)
    
    for(cluster in unique(fsom$metaclustering)){
      if (sum(cellClusterIDs[[file]] == cluster) > 0) {
        f <- file.path(outputDir,
                       paste0(gsub("[:/]","_",file),
                              "_fsom", cluster, ".fcs"))
        suppressWarnings(
          flowCore::write.FCS(ff[cellClusterIDs[[file]] == cluster],
                              file = f)
        )
      }
    }
  }
  
  # Apply normalization on each cluster
  for(cluster in unique(fsom$metaclustering)){
    if(verbose) message("Processing cluster ",cluster)
    files_tmp <- file.path(outputDir,
                           paste0(gsub("[:/]",
                                       "_",
                                       files),
                                  "_fsom",
                                  cluster,
                                  ".fcs"))
    labels_tmp <- labels[file.exists(files_tmp)]
    files_tmp <- files_tmp[file.exists(files_tmp)]
    normMethod.normalize(model = clusterRes[[cluster]],
                         files = files_tmp,
                         labels = labels_tmp,
                         outputDir = file.path(outputDir),
                         prefix = "Norm_",
                         transformList = NULL,
                         transformList.reverse = NULL,
                         removeOriginal = TRUE,
                         verbose = verbose)
  }
  
  # Combine clusters into one final fcs file
  for(file in files){
    if(verbose) message("Rebuilding ",file)
    
    ff <- flowCore::read.FCS(file)
    
    # Addition of this code to manage infinite values
    list_channels<-channels
    for (channel in list_channels){
      ff@exprs[,channel][mapply(is.infinite, ff@exprs[,channel])] <-quantile(ff@exprs[,channel], probs = .99)
    }
    
    for(cluster in unique(fsom$metaclustering)){
      file_name <- file.path(outputDir,
                             paste0("Norm_",gsub("[:/]","_",file),
                                    "_fsom",cluster,".fcs"))
      if (file.exists(file_name)) {
        ff_subset <- flowCore::read.FCS(file_name)
        flowCore::exprs(ff)[cellClusterIDs[[file]] == cluster,] <- flowCore::exprs(ff_subset)
      }
    }
    
    if(!is.null(transformList.reverse)){
      ff <- flowCore::transform(ff, transformList.reverse)
      # ff@description <- meta[[file]][["description_original"]]
      # ff@parameters <- meta[[file]][["parameters_original"]]
    }
    
    
    # Adapt to real min and max because this gets strange values otherwise
    ff@parameters@data[,"minRange"] <- apply(ff@exprs, 2, min)
    ff@parameters@data[,"maxRange"] <- apply(ff@exprs, 2, max)
    ff@parameters@data[,"range"] <- ff@parameters@data[,"maxRange"] -
      ff@parameters@data[,"minRange"]
    
    if(clean){
      file.remove(file.path(outputDir,
                            paste0("Norm_",gsub("[:/]","_",file),
                                   "_fsom",unique(fsom$metaclustering),".fcs")))
    }
    
    suppressWarnings(flowCore::write.FCS(ff,
                                         file=file.path(outputDir,
                                                        paste0(prefix,gsub(".*/","",file)))))
  }
}


############################## read FCS files

# get the input from tercen
ctx <- tercenCtx()
task<-ctx$task
nclust <- as.double(ctx$op.value('cluster'))
ncells <- as.double(ctx$op.value('number_of_cells'))

batch<-ctx$select(unlist(list(ctx$colors, '.ci')))  %>% 
  group_by(.ci)  %>%
  unique(.)

labels<-ctx$select(unlist(list(ctx$labels, '.ci')))  %>% 
  group_by(.ci) %>% 
  unique(.)

#option set workflow default option set in tutorial
nclust <- 10
ncells <- 6000

data_all <-as.matrix(ctx) %>% t()
colnames(data_all) <- ctx$rselect()[[1]]
data_all <-cbind(data_all, labels[,1], batch[,1], ctx$cselect())

long_data<-pivot_longer(data_all,cols = -c("js1.Type","js1.batch","js1.filename","Time"))
colnames(long_data)<-c("js1.Type","js1.batch","js1.filename","Time","gs3.variable","gs3.value")
write.csv(long_data, file="./tests/input_long.csv",  row.names = FALSE)


chan_nb <- length(ctx$rselect()[[1]])

colnames(data_all)[grep("[T,t]ype",colnames(data_all))]<-"type"
colnames(data_all)

colnames(data_all)[grep("[F,f]ilename",colnames(data_all))]<-"filename"
colnames(data_all)

colnames(data_all)[grep("[B,b]atch",colnames(data_all))]<-"batch"
colnames(data_all)

train_data <- data_all[data_all["type"]== "Train",]
validate_data <- data_all[data_all["type"]== "validate",]
batch_train_data <- unique(data_all[data_all["type"]== "Train",]$batch)
batch_validate_data <- unique(data_all[data_all["type"]== "validate",]$batch)

#create temporary file 

dir.create("train")
for (filename in unique(train_data$"filename"))     {
  tmp_train_file_data <- train_data[train_data["filename"] == filename,]
  
  #write in the data file the channels without the annotation columns but with the time 
  flow.dat <- flowCore::flowFrame(as.matrix(tmp_train_file_data[c(1:(chan_nb))]))
  outfile<-paste("train/",filename, sep="")
  write.FCS(flow.dat, outfile)
}

dir.create("validate")
for (filename in unique(validate_data$"filename"))     {
  tmp_val_file_data <- validate_data[validate_data["filename"] == filename,]
  #write in the data file the channels without the annotation columns but with the time 
  flow.dat <- flowCore::flowFrame(as.matrix(tmp_val_file_data[c(1:(chan_nb))]))
  outfile<-paste("validate/",filename, sep="")
  write.FCS(flow.dat, outfile)
}

# Open 1 of the flow cytometry files to get the channels and set up the tranformation
#channels <- flowCore::colnames(train_data)[c(1:chan_nb)]

#channels <- colnames(train_data)[c(48, 46, 43, 45, 20, 16, 21, 19, 22, 50, 47,
                                     # 40, 44, 33, 17, 11, 18, 51, 14, 23, 32, 10,
                                     # 49, 27, 24, 31, 42, 37, 39, 34, 41, 26, 30, 
                                     # 28, 29, 25, 35)-3]
#channels <- colnames(train_data)[c(52,26,34,53,12,48,46,8,33,43,13,18,30,23,47,16,28,22,45,41,14,21,15,37,39,17,38,42,50,31,10)]
#channels <- colnames(validate_data)[c(52,26,34,53,12,48,46,8,33,43,13,18,30,23,47,16,28,22,45,41,14,21,15,37,39,17,38,42,50,31,10)]
#channels <- colnames(validate_data)[c(1:20)]
channels <- colnames(train_data)[c(0:chan_nb)]

transformList <- flowCore::transformList(channels,cytofTransform)
transformList.reverse <- flowCore::transformList(channels,cytofTransform.reverse)

############################ Preparation 
list_train<-list.files("train",full.names = TRUE)
list_validate<-list.files("validate",full.names = TRUE)

fsom <- prepareFlowSOM(list_train,
                       channels,
                       nCells = ncells,
                       FlowSOM.params = list(xdim = 5,
                                             ydim = 5,
                                             nClus = nclust,
                                             scale = FALSE),
                       transformList = transformList,
                       seed = 1)

########################### Model determination
model <- CytoNorm.train(files = list_train,
                        labels =  batch_train_data,
                        channels = channels,
                        transformList = transformList,
                        FlowSOM.params = list(nCells = ncells, 
                                              xdim = 5,
                                              ydim = 5,
                                              nClus = nclust,
                                              scale = FALSE),
                        normMethod.train = QuantileNorm.train,
                        normParams = list(nQ = 101,
                                          goal = "mean"),
                        seed = 1,
                        verbose = TRUE)
########################## Application of CytoNorm

CytoNorm.normalize.custom(model = model,
                   files = list_validate,
                   labels = batch_validate_data,
                   channels = channels,
                   transformList = transformList,
                   transformList.reverse = transformList.reverse,
                   normMethod.normalize = QuantileNorm.normalize,
                   outputDir = "Normalized",
                   prefix = "Norm_",
                   clean = TRUE,
                   verbose = TRUE)

############################# Output

f.names<- paste("./Normalized/",list.files(path="./Normalized", pattern="Norm_"), sep="")
test.fun<-f.names%>%
  lapply(function(filename){
    data = fcs_to_data(filename)
    if (!is.null(task)) {
      # task is null when run from RStudio
      actual <-get("actual",  envir = .GlobalEnv) + 1
      assign("actual", actual, envir = .GlobalEnv)
      evt = TaskProgressEvent$new()
      evt$taskId = task$id
      evt$total = length(f.names)
      evt$actual = actual
      evt$message = paste0('processing FCS file ' , filename)
      ctx$client$eventService$sendChannel(task$channelId, evt)
    } else {
      cat('processing FCS file ' , filename)
    }
    data
  }) %>%
  bind_rows() 


test.fun.long<-test.fun%>%
  #ctx$addNamespace()  %>%
  pivot_longer(., cols =channels)

colnames(test.fun.long)[3]<-"variable"

ctx$addNamespace(test.fun.long) %>%
  ctx$save()

unlink("train",recursive = TRUE)
unlink("validate",recursive = TRUE)
unlink("Normalized",recursive = TRUE)

