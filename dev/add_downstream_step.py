"""Add a DataStep downstream of an existing one, projected and ready for the dev/production run.

    TERCEN_TOKEN=… python dev/add_downstream_step.py <workflowId> <upstreamStepId> \
        <rowFactor> <colFactor> <yFactor> [prop=value …]

The upstream step must have run: its `computedRelation` is what the CubeQuery reads.
"""
import os, sys, uuid, json
import tercen.model.impl as m
from tercen.client.factory import TercenClient

tok = os.environ["TERCEN_TOKEN"]
wf_id, up_id, ROW, COL, Y = sys.argv[1:6]
props = dict(a.split("=", 1) for a in sys.argv[6:])

c = TercenClient(os.environ.get("TERCEN_HTTP", "http://127.0.0.1:5402"))
c.userService.tercenClient.token = tok
c.httpClient.authorization = tok
wf = c.workflowService.get(wf_id)
up = next(s for s in wf.steps if s.id == up_id)

def rect(x, y, w=200.0, h=55.0):
    r = m.Rectangle(); r.topLeft = m.Point(); r.topLeft.x = x; r.topLeft.y = y
    r.extent = m.Point(); r.extent.x = w; r.extent.y = h; return r
def factor(n, t):
    f = m.Factor(); f.name = n; f.type = t; return f
def gf(n, t):
    g = m.GraphicalFactor(); g.factor = factor(n, t)
    g.rectangle = rect(0.0, 0.0, max(len(n) * 10.0, 30.0), 30.0); return g
def ctable(fs):
    t = m.CrosstabTable(); t.cellSize = 250.0; t.offset = 0; t.nRows = 0
    t.graphicalFactors = fs; t.rectangleSelections = []; return t

ds = m.DataStep(); ds.id = str(uuid.uuid4()); ds.name = "asinh (Rust, dev)"
ds.groupId = ""; ds.description = ""; ds.parentDataStepId = ""
ip = m.InputPort(); ip.id = str(uuid.uuid4()); ip.name = "data"; ip.linkType = "relation"
op = m.OutputPort(); op.id = str(uuid.uuid4()); op.name = "data"; op.linkType = "relation"
ds.inputs = [ip]; ds.outputs = [op]
ds.rectangle = rect(up.rectangle.topLeft.x, up.rectangle.topLeft.y + 150.0)
ds.state = m.StepState(); ds.state.taskId = ""; ds.state.taskState = m.InitState()
ct = m.Crosstab(); ct.taskId = ""
ct.axis = m.XYAxisList(); ct.axis.rectangleSelections = []; ct.axis.xyAxis = []
ct.columnTable = ctable([gf(COL, "double")])
ct.rowTable = ctable([gf(ROW, "string")])
ct.filters = m.Filters(); ct.filters.removeNaN = False; ct.filters.namedFilters = []
st = m.OperatorSettings(); st.namespace = "ds1"; st.environment = []
ref = m.OperatorRef(); ref.name = "asinh_rust_operator"; ref.version = "dev"
ref.operatorId = ""; ref.operatorKind = ""
ref.url = m.Url(); ref.url.uri = "https://github.com/tercen/asinh_rust_operator"
ref.propertyValues = []
for k, v in props.items():
    pv = m.PropertyValue(); pv.name = k; pv.value = v; ref.propertyValues.append(pv)
ref.operatorSpec = m.OperatorSpec(); ref.operatorSpec.inputSpecs = []; ref.operatorSpec.outputSpecs = []
st.operatorRef = ref
ct.operatorSettings = st
ds.model = ct

link = m.Link(); link.id = str(uuid.uuid4()); link.inputId = ip.id; link.outputId = up.outputs[0].id
wf.steps = list(wf.steps) + [ds]; wf.links = list(wf.links) + [link]
c.workflowService.update(wf)
print("DATA_STEP", ds.id)

# CubeQueryTask over the upstream result
wf = c.workflowService.get(wf_id)
q = m.CubeQuery()
q.relation = up.computedRelation
q.colColumns = [factor(COL, "double")]
q.rowColumns = [factor(ROW, "string")]
aq = m.CubeAxisQuery(); aq.chartType = "point"; aq.pointSize = 4
aq.xAxis = factor("", "string"); aq.yAxis = factor(Y, "double")
aq.colors = []; aq.errors = []; aq.labels = []; aq.preprocessors = []
aq.xAxisSettings = m.AxisSettings(); aq.xAxisSettings.meta = []
aq.yAxisSettings = m.AxisSettings(); aq.yAxisSettings.meta = []
q.axisQueries = [aq]
q.filters = ds.model.filters
q.operatorSettings = ds.model.operatorSettings
task = m.CubeQueryTask(); task.state = m.InitState(); task.owner = wf.acl.owner
task.projectId = wf.projectId; task.query = q
task = c.taskService.create(task)
c.taskService.runTask(task.id)
task = c.taskService.waitDone(task.id)
print("CUBE_QUERY_TASK", task.id, type(task.state).__name__, getattr(task.state, "reason", ""))
if type(task.state).__name__ != "DoneState":
    sys.exit(1)
axis = json.loads(open(os.path.join(os.path.dirname(__file__), "axis_template.json")).read())
axis["xyAxis"][0]["taskId"] = task.id
axis["xyAxis"][0]["yAxis"]["graphicalFactor"]["factor"]["name"] = Y
wf = c.workflowService.get(wf_id)
ds2 = next(s for s in wf.steps if s.id == ds.id)
ds2.model.taskId = task.id
ds2.model.axis = m.XYAxisList(axis)
c.workflowService.update(wf)
print("READY", wf_id, ds.id, "schema ids", len(task.schemaIds))
